//! Aerodrome Finance Substreams for Base.
//!
//! Covers both Aerodrome venues:
//!
//! * **v2** -- the volatile/stable AMM (`Swap`, `Mint`, `Burn`, `Sync`)
//! * **Slipstream** -- concentrated liquidity (`Swap`, `Mint`, `Burn`, `Collect`),
//!   which is where Coinbase B20 tokenized stocks trade
//!
//! Pools are discovered from factory `PoolCreated` events rather than from a
//! hardcoded list, and any pool with a B20 token on one side is flagged. New
//! tickers are picked up automatically.
//!
//! # Why every handler gates on the pool registry
//!
//! Aerodrome shares event signatures with the protocols it descends from, so a
//! topic match alone does not mean the log came from an Aerodrome pool:
//!
//! * Slipstream's `Swap` is byte-identical to Uniswap V3's, and so is every V3
//!   fork on Base.
//! * v2's `Mint(address,uint256,uint256)` is byte-identical to Uniswap V2's.
//!   Sampling Base for that topic, only 8 of 20 emitters were Aerodrome pools --
//!   an ungated handler misattributes the rest.
//! * v2's `Burn` collides in the same way.
//!
//! Membership in the factory-built registry is what actually identifies a pool,
//! so a log from an unknown pool is dropped rather than emitted unenriched. The
//! registry starts at the v2 `PoolFactory` deployment and is therefore complete.
//! Raising `initialBlock` above that trades completeness for backfill speed:
//! pools created before the new start are unknown, and their events are skipped.

mod abi;
pub mod b20;
mod pb;
mod b20_events;
mod pools;
mod stores;
mod stock_list;
pub mod stocks;
mod gauges;
pub use gauges::map_gauge_events;
pub use stocks::{map_stock_swaps, store_stock_daily_volume, store_stock_registry};

pub use b20_events::map_b20_events;
pub use pools::{map_factories, map_pools, store_factories};
pub use stores::{
    store_cl_volumes, store_pool_stats, store_pools, store_swap_volumes, store_unique_traders,
};

use abi::cl_pool;
use abi::pool::events::{Burn, Mint, Swap, Sync};
use pb::aerodrome::{
    AerodromeLiquidity, AerodromeSwap, B20Events, ClLiquidity, ClLiquidityEvents, ClSwap, ClSwaps,
    LiquidityEvents, Pool, SwapEvents, SyncEvent, SyncEvents,
};
use std::str::FromStr;
use substreams::scalar::BigInt;
use substreams::store::{DeltaBigDecimal, Deltas, StoreGet, StoreGetProto, StoreGetString};
use substreams::Hex;
use substreams_database_change::pb::database::DatabaseChanges;
use substreams_database_change::tables::Tables;
use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::Event;

fn block_timestamp(blk: &eth::Block) -> u64 {
    blk.header.timestamp.seconds as u64
}

fn lookup_pool(store: &StoreGetProto<Pool>, pool_address: &str) -> Option<Pool> {
    store.get_last(stores::pool_key(pool_address))
}

/// `BigInt::to_u64` panics on overflow (v0.3.2 halted at block 13,847,696 on a
/// v2 swap whose summed amounts exceed u64). Saturate instead.
pub(crate) fn sat_u64(value: &BigInt) -> u64 {
    if value.lt(&BigInt::zero()) {
        return 0;
    }
    value.to_string().parse::<u64>().unwrap_or(u64::MAX)
}

fn big(value: &str) -> BigInt {
    BigInt::from_str(value).unwrap_or_else(|_| BigInt::zero())
}

/// Price of token0 quoted in token1, in raw (undecimalized) units.
///
/// Taken from the magnitudes of the two net amounts, so a buy and a sell at the
/// same price produce the same number. Dividing output by input instead would
/// yield the price for one direction and its reciprocal for the other, which is
/// what makes a naively built candle series alternate between a price and its
/// inverse. Apply `token0_decimals` / `token1_decimals` from the `pools` table to
/// get a human price.
fn v2_price_token1_per_token0(swap: &AerodromeSwap) -> String {
    let amount0 = (big(&swap.amount0_in) - big(&swap.amount0_out)).absolute();
    let amount1 = (big(&swap.amount1_in) - big(&swap.amount1_out)).absolute();

    if amount0 == BigInt::zero() || amount1 == BigInt::zero() {
        return String::new();
    }

    (amount1.to_decimal(0) / amount0.to_decimal(0)).to_string()
}

// ---------------------------------------------------------------------------
// Aerodrome v2
// ---------------------------------------------------------------------------

/// Extracts v2 swaps, enriched from the pool registry.
///
/// Like every other handler here, a log from a pool that is not in the registry
/// is skipped. See the module docs for why that gate is load-bearing rather than
/// merely tidy.
#[substreams::handlers::map]
pub fn map_swaps(
    blk: eth::Block,
    pool_store: StoreGetProto<Pool>,
) -> Result<SwapEvents, substreams::errors::Error> {
    let timestamp = block_timestamp(&blk);
    let mut swaps: Vec<AerodromeSwap> = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();

        for log in &receipt.receipt.logs {
            let Some(swap) = Swap::match_and_decode(log) else {
                continue;
            };

            let pool_address = Hex(&log.address).to_string();
            let Some(pool) = lookup_pool(&pool_store, &pool_address) else {
                continue; // not an Aerodrome pool
            };

            swaps.push(AerodromeSwap {
                block_number: blk.number,
                transaction_hash: tx_hash.clone(),
                log_index: log.index as u64,
                pool_address,
                sender: Hex(&swap.sender).to_string(),
                recipient: Hex(&swap.to).to_string(),
                amount0_in: swap.amount0_in.to_string(),
                amount1_in: swap.amount1_in.to_string(),
                amount0_out: swap.amount0_out.to_string(),
                amount1_out: swap.amount1_out.to_string(),
                timestamp,
                token0: pool.token0,
                token1: pool.token1,
                is_b20: pool.is_b20,
                b20_token: pool.b20_token,
                pool_type: pool.pool_type,
            });
        }
    }

    let swap_count = swaps.len() as u32;
    let b20_swap_count = swaps.iter().filter(|s| s.is_b20).count() as u32;
    // Kept for backward compatibility; saturates rather than truncating.
    let total_volume = swaps
        .iter()
        .fold(BigInt::zero(), |acc, s| {
            acc + big(&s.amount0_in) + big(&s.amount1_in)
        });
    let total_volume = sat_u64(&total_volume);

    Ok(SwapEvents {
        swaps,
        swap_count,
        total_volume,
        b20_swap_count,
    })
}

/// Extracts v2 liquidity events (`Mint` / `Burn`).
#[substreams::handlers::map]
pub fn map_liquidity(
    blk: eth::Block,
    pool_store: StoreGetProto<Pool>,
) -> Result<LiquidityEvents, substreams::errors::Error> {
    let timestamp = block_timestamp(&blk);
    let mut events = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();

        for log in &receipt.receipt.logs {
            let pool_address = Hex(&log.address).to_string();

            // Aerodrome's Mint carries no recipient; the sender is the LP.
            if let Some(mint) = Mint::match_and_decode(log) {
                let Some(pool) = lookup_pool(&pool_store, &pool_address) else {
                    continue;
                };
                events.push(AerodromeLiquidity {
                    block_number: blk.number,
                    transaction_hash: tx_hash.clone(),
                    log_index: log.index as u64,
                    pool_address: pool_address.clone(),
                    sender: Hex(&mint.sender).to_string(),
                    recipient: String::new(),
                    amount0: mint.amount0.to_string(),
                    amount1: mint.amount1.to_string(),
                    action: "mint".to_string(),
                    timestamp,
                    token0: pool.token0,
                    token1: pool.token1,
                    is_b20: pool.is_b20,
                    b20_token: pool.b20_token,
                });
                continue;
            }

            if let Some(burn) = Burn::match_and_decode(log) {
                let Some(pool) = lookup_pool(&pool_store, &pool_address) else {
                    continue;
                };
                events.push(AerodromeLiquidity {
                    block_number: blk.number,
                    transaction_hash: tx_hash.clone(),
                    log_index: log.index as u64,
                    pool_address,
                    sender: Hex(&burn.sender).to_string(),
                    recipient: Hex(&burn.to).to_string(),
                    amount0: burn.amount0.to_string(),
                    amount1: burn.amount1.to_string(),
                    action: "burn".to_string(),
                    timestamp,
                    token0: pool.token0,
                    token1: pool.token1,
                    is_b20: pool.is_b20,
                    b20_token: pool.b20_token,
                });
            }
        }
    }

    let event_count = events.len() as u32;
    Ok(LiquidityEvents {
        events,
        event_count,
    })
}

/// Extracts v2 `Sync` events (post-trade reserves).
#[substreams::handlers::map]
pub fn map_syncs(
    blk: eth::Block,
    pool_store: StoreGetProto<Pool>,
) -> Result<SyncEvents, substreams::errors::Error> {
    let timestamp = block_timestamp(&blk);
    let mut syncs: Vec<SyncEvent> = Vec::new();

    for receipt in blk.receipts() {
        for log in &receipt.receipt.logs {
            let Some(sync) = Sync::match_and_decode(log) else {
                continue;
            };

            let pool_address = Hex(&log.address).to_string();
            let Some(pool) = lookup_pool(&pool_store, &pool_address) else {
                continue; // not an Aerodrome pool
            };

            syncs.push(SyncEvent {
                block_number: blk.number,
                pool_address,
                reserve0: sync.reserve0.to_string(),
                reserve1: sync.reserve1.to_string(),
                timestamp,
                token0: pool.token0,
                token1: pool.token1,
                is_b20: pool.is_b20,
            });
        }
    }

    let event_count = syncs.len() as u32;
    Ok(SyncEvents {
        events: syncs,
        event_count,
    })
}

// ---------------------------------------------------------------------------
// Slipstream concentrated liquidity
// ---------------------------------------------------------------------------

/// Extracts Slipstream swaps -- the venue where tokenized stocks trade.
///
/// Slipstream inherits Uniswap V3's `Swap` signature verbatim, so the topic alone
/// cannot tell an Aerodrome pool from any other V3-style pool on Base. Membership
/// in the factory-built pool registry is what makes the distinction, which is why
/// a log from an unknown pool is skipped rather than emitted unenriched.
#[substreams::handlers::map]
pub fn map_cl_swaps(
    blk: eth::Block,
    pool_store: StoreGetProto<Pool>,
) -> Result<ClSwaps, substreams::errors::Error> {
    let timestamp = block_timestamp(&blk);
    let mut swaps: Vec<ClSwap> = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();

        for log in &receipt.receipt.logs {
            let Some(swap) = cl_pool::events::Swap::match_and_decode(log) else {
                continue;
            };

            let pool_address = Hex(&log.address).to_string();
            let Some(pool) = lookup_pool(&pool_store, &pool_address) else {
                continue; // not an Aerodrome pool
            };

            swaps.push(ClSwap {
                block_number: blk.number,
                transaction_hash: tx_hash.clone(),
                log_index: log.index as u64,
                pool_address,
                sender: Hex(&swap.sender).to_string(),
                recipient: Hex(&swap.recipient).to_string(),
                amount0: swap.amount0.to_string(),
                amount1: swap.amount1.to_string(),
                sqrt_price_x96: swap.sqrt_price_x96.to_string(),
                liquidity: swap.liquidity.to_string(),
                tick: swap.tick.to_i32(),
                timestamp,
                token0: pool.token0,
                token1: pool.token1,
                tick_spacing: pool.tick_spacing,
                is_b20: pool.is_b20,
                b20_token: pool.b20_token,
            });
        }
    }

    let count = swaps.len() as u32;
    let b20_count = swaps.iter().filter(|s| s.is_b20).count() as u32;

    Ok(ClSwaps {
        swaps,
        count,
        b20_count,
    })
}

/// Extracts Slipstream position events (`Mint` / `Burn` / `Collect`).
///
/// `owner` is the position holder as the pool sees it. For positions opened
/// through the NonfungiblePositionManager that is the manager contract, and the
/// end user is the owner of the corresponding position NFT.
#[substreams::handlers::map]
pub fn map_cl_liquidity(
    blk: eth::Block,
    pool_store: StoreGetProto<Pool>,
) -> Result<ClLiquidityEvents, substreams::errors::Error> {
    let timestamp = block_timestamp(&blk);
    let mut events: Vec<ClLiquidity> = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();

        for log in &receipt.receipt.logs {
            let pool_address = Hex(&log.address).to_string();

            let decoded: Option<ClLiquidity> = if let Some(m) =
                cl_pool::events::Mint::match_and_decode(log)
            {
                Some(ClLiquidity {
                    action: "mint".to_string(),
                    owner: Hex(&m.owner).to_string(),
                    sender: Hex(&m.sender).to_string(),
                    recipient: String::new(),
                    tick_lower: m.tick_lower.to_i32(),
                    tick_upper: m.tick_upper.to_i32(),
                    amount: m.amount.to_string(),
                    amount0: m.amount0.to_string(),
                    amount1: m.amount1.to_string(),
                    ..Default::default()
                })
            } else if let Some(b) = cl_pool::events::Burn::match_and_decode(log) {
                Some(ClLiquidity {
                    action: "burn".to_string(),
                    owner: Hex(&b.owner).to_string(),
                    sender: String::new(),
                    recipient: String::new(),
                    tick_lower: b.tick_lower.to_i32(),
                    tick_upper: b.tick_upper.to_i32(),
                    amount: b.amount.to_string(),
                    amount0: b.amount0.to_string(),
                    amount1: b.amount1.to_string(),
                    ..Default::default()
                })
            } else if let Some(c) = cl_pool::events::Collect::match_and_decode(log) {
                Some(ClLiquidity {
                    action: "collect".to_string(),
                    owner: Hex(&c.owner).to_string(),
                    sender: String::new(),
                    recipient: Hex(&c.recipient).to_string(),
                    tick_lower: c.tick_lower.to_i32(),
                    tick_upper: c.tick_upper.to_i32(),
                    amount: String::new(),
                    amount0: c.amount0.to_string(),
                    amount1: c.amount1.to_string(),
                    ..Default::default()
                })
            } else {
                None
            };

            let Some(mut event) = decoded else { continue };
            let Some(pool) = lookup_pool(&pool_store, &pool_address) else {
                continue; // not an Aerodrome pool
            };

            event.block_number = blk.number;
            event.transaction_hash = tx_hash.clone();
            event.log_index = log.index as u64;
            event.pool_address = pool_address;
            event.timestamp = timestamp;
            event.token0 = pool.token0;
            event.token1 = pool.token1;
            event.is_b20 = pool.is_b20;
            event.b20_token = pool.b20_token;

            events.push(event);
        }
    }

    let count = events.len() as u32;
    Ok(ClLiquidityEvents { events, count })
}

// ---------------------------------------------------------------------------
// SQL sink
// ---------------------------------------------------------------------------

/// Emits CDC rows for the pool registry, v2 swaps, Slipstream swaps and
/// Slipstream liquidity. Aggregations (candles, per-pool and per-stock stats)
/// are derived in SQL -- see `schema.sql` and `schema.clickhouse.sql`.
#[substreams::handlers::map]
pub fn db_out(
    pools: pb::aerodrome::Pools,
    swaps: SwapEvents,
    cl_swaps: ClSwaps,
    cl_liquidity: ClLiquidityEvents,
    b20: B20Events,
    stock_swaps: pb::aerodrome::StockSwaps,
    stock_daily: Deltas<DeltaBigDecimal>,
    stock_registry: StoreGetString,
    gauges: pb::aerodrome::GaugeEvents,
) -> Result<DatabaseChanges, substreams::errors::Error> {
    let mut tables = Tables::new();

    for pool in &pools.pools {
        tables
            .create_row("pools", &pool.address)
            .set("address", &pool.address)
            .set("token0", &pool.token0)
            .set("token1", &pool.token1)
            .set("pool_type", &pool.pool_type)
            .set("tick_spacing", pool.tick_spacing)
            .set("factory", &pool.factory)
            .set("is_b20", pool.is_b20)
            .set("b20_token", &pool.b20_token)
            .set("b20_variant", &pool.b20_variant)
            .set("created_block", pool.created_block)
            .set("created_timestamp", pool.created_timestamp as i64)
            .set("created_tx", &pool.created_tx)
            .set("token0_decimals", pool.token0_decimals)
            .set("token1_decimals", pool.token1_decimals)
            .set("token0_symbol", &pool.token0_symbol)
            .set("token1_symbol", &pool.token1_symbol)
            .set("is_stock", pool.is_stock)
            .set("stock_ticker", &pool.stock_ticker);
    }

    for swap in &swaps.swaps {
        let swap_id = format!("{}:{}", swap.transaction_hash, swap.log_index);
        let amount_in = big(&swap.amount0_in) + big(&swap.amount1_in);
        let amount_out = big(&swap.amount0_out) + big(&swap.amount1_out);

        tables
            .create_row("aerodrome_swaps", &swap_id)
            .set("tx_hash", &swap.transaction_hash)
            .set("log_index", swap.log_index)
            .set("block_number", swap.block_number)
            .set("timestamp", swap.timestamp as i64)
            .set("pool_address", &swap.pool_address)
            .set("sender", &swap.sender)
            .set("recipient", &swap.recipient)
            .set("amount0_in", &swap.amount0_in)
            .set("amount1_in", &swap.amount1_in)
            .set("amount0_out", &swap.amount0_out)
            .set("amount1_out", &swap.amount1_out)
            .set("amount_in_total", amount_in.to_string())
            .set("amount_out_total", amount_out.to_string())
            .set("price_token1_per_token0", v2_price_token1_per_token0(swap))
            .set("token0", &swap.token0)
            .set("token1", &swap.token1)
            .set("pool_type", &swap.pool_type)
            .set("is_b20", swap.is_b20)
            .set("b20_token", &swap.b20_token);
    }

    for swap in &cl_swaps.swaps {
        let swap_id = format!("{}:{}", swap.transaction_hash, swap.log_index);

        tables
            .create_row("aerodrome_cl_swaps", &swap_id)
            .set("tx_hash", &swap.transaction_hash)
            .set("log_index", swap.log_index)
            .set("block_number", swap.block_number)
            .set("timestamp", swap.timestamp as i64)
            .set("pool_address", &swap.pool_address)
            .set("sender", &swap.sender)
            .set("recipient", &swap.recipient)
            .set("amount0", &swap.amount0)
            .set("amount1", &swap.amount1)
            .set("sqrt_price_x96", &swap.sqrt_price_x96)
            .set("liquidity", &swap.liquidity)
            .set("tick", swap.tick)
            .set("token0", &swap.token0)
            .set("token1", &swap.token1)
            .set("tick_spacing", swap.tick_spacing)
            .set("is_b20", swap.is_b20)
            .set("b20_token", &swap.b20_token);
    }

    for event in &cl_liquidity.events {
        let event_id = format!("{}:{}", event.transaction_hash, event.log_index);

        let row = tables.create_row("aerodrome_cl_liquidity", &event_id);
        // `amount` (liquidity delta) is empty for Collect; NULL instead of an
        // invalid empty NUMERIC string.
        if !event.amount.is_empty() {
            row.set("amount", &event.amount);
        }
        row
            .set("tx_hash", &event.transaction_hash)
            .set("log_index", event.log_index)
            .set("block_number", event.block_number)
            .set("timestamp", event.timestamp as i64)
            .set("pool_address", &event.pool_address)
            .set("action", &event.action)
            .set("owner", &event.owner)
            .set("sender", &event.sender)
            .set("recipient", &event.recipient)
            .set("tick_lower", event.tick_lower)
            .set("tick_upper", event.tick_upper)
            .set("amount0", &event.amount0)
            .set("amount1", &event.amount1)
            .set("token0", &event.token0)
            .set("token1", &event.token1)
            .set("is_b20", event.is_b20)
            .set("b20_token", &event.b20_token);
    }

    for event in &b20.events {
        let event_id = format!("{}:{}", event.transaction_hash, event.log_index);

        let row = tables.create_row("b20_events", &event_id);
        // NUMERIC columns: an empty string is not a valid NUMERIC and halts the
        // SQL sink (v0.3.x bug), so they are left NULL when the event has no value.
        for (col, v) in [("amount", &event.amount), ("old_value", &event.old_value), ("new_value", &event.new_value)] {
            if !v.is_empty() {
                row.set(col, v);
            }
        }
        row
            .set("tx_hash", &event.transaction_hash)
            .set("log_index", event.log_index)
            .set("block_number", event.block_number)
            .set("timestamp", event.timestamp as i64)
            .set("token", &event.token)
            .set("variant", &event.variant)
            .set("event_type", &event.event_type)
            .set("actor", &event.actor)
            .set("account", &event.account)
            .set("recipient", &event.recipient)
            .set("effective_at", event.effective_at as i64)
            .set("key", &event.key)
            .set("text", &event.text)
            .set("announcement_id", &event.announcement_id)
            .set("uri", &event.uri)
            .set("in_announcement", event.in_announcement)
            .set("pause_features", &event.pause_features);

        // `B20Created` is the token's birth certificate. Renames are NOT folded
        // into this table: every B20 that existed before the factory started
        // emitting the event has no row here, and an update against a missing row
        // is not a safe thing to emit. The current name and symbol are resolved
        // in SQL from the `symbol_updated` / `name_updated` history instead --
        // see the `b20_token_current` view.
        if event.event_type == "b20_created" {
            tables
                .create_row("b20_tokens", &event.token)
                .set("address", &event.token)
                .set("variant", &event.variant)
                .set("name", &event.key)
                .set("symbol", &event.text)
                .set("decimals", &event.new_value)
                .set("created_block", event.block_number)
                .set("created_timestamp", event.timestamp as i64)
                .set("deployer", &event.actor);
        }
    }

    for change in &b20.multiplier_changes {
        let change_id = format!("{}:{}", change.transaction_hash, change.log_index);

        let row = tables.create_row("b20_multiplier_schedule", &change_id);
        for (col, v) in [("old_multiplier", &change.old_multiplier), ("new_multiplier", &change.new_multiplier)] {
            if !v.is_empty() {
                row.set(col, v);
            }
        }
        row
            .set("token", &change.token)
            .set("effective_at", change.effective_at as i64)
            .set("announced_block", change.announced_block)
            .set("announced_timestamp", change.announced_timestamp as i64)
            .set("tx_hash", &change.transaction_hash)
            .set("log_index", change.log_index)
            .set("scheduled", change.scheduled)
            .set("cancelled", change.cancelled)
            .set("announcement_id", &change.announcement_id);
    }

    // -- v1.0.0: tokenized stocks ------------------------------------------
    for sw in &stock_swaps.swaps {
        let id = format!("{}:{}", sw.transaction_hash, sw.log_index);
        let row = tables.create_row("stock_trades", &id);
        row
            .set("tx_hash", &sw.transaction_hash)
            .set("log_index", sw.log_index)
            .set("block_number", sw.block_number)
            .set("timestamp", sw.timestamp as i64)
            .set("pool_address", &sw.pool_address)
            .set("pool_type", &sw.pool_type)
            .set("ticker", &sw.ticker)
            .set("stock_token", &sw.stock_token)
            .set("quote_token", &sw.quote_token)
            .set("quote_symbol", &sw.quote_symbol)
            .set("side", &sw.side)
            .set("stock_amount_raw", &sw.stock_amount_raw)
            .set("quote_amount_raw", &sw.quote_amount_raw)
            .set("stock_amount", &sw.stock_amount)
            .set("quote_amount", &sw.quote_amount)
            .set("multiplier", &sw.multiplier)
            .set("share_amount", &sw.share_amount)
            .set("sender", &sw.sender)
            .set("recipient", &sw.recipient);
        // USD columns stay NULL unless the quote is native USDC.
        if !sw.usd_notional.is_empty() {
            row.set("usd_notional", &sw.usd_notional);
        }
        if !sw.usd_price_per_token.is_empty() {
            row.set("usd_price_per_token", &sw.usd_price_per_token);
        }
        if !sw.usd_price_per_share.is_empty() {
            row.set("usd_price_per_share", &sw.usd_price_per_share);
        }
    }

    // Running per-day totals; the store holds the authoritative value, so each
    // delta is an idempotent upsert (works on Postgres and ClickHouse).
    for d in &stock_daily.deltas {
        let mut it = d.key.rsplitn(2, '|');
        let (Some(metric), Some(rest)) = (it.next(), it.next()) else { continue };
        let Some((day, ticker)) = rest.split_once('|') else { continue };
        let col = match metric {
            "usd" => "volume_usd",
            "tokens" => "volume_tokens",
            "shares" => "volume_shares",
            "count" => "swap_count",
            "usd_count" => "usd_swap_count",
            _ => continue,
        };
        tables
            .upsert_row("stock_daily_volume", format!("{}|{}", day, ticker))
            .set("day", day)
            .set("ticker", ticker)
            .set(col, d.new_value.to_string());
    }

    for ch in &b20.multiplier_changes {
        let Some(ticker) = stocks::resolve_ticker(&ch.token, &[], &stock_registry) else { continue };
        let kind = if ch.cancelled { "cancelled" } else if ch.scheduled { "scheduled" } else { "instant" };
        tables
            .create_row("stock_corporate_actions", format!("{}:{}", ch.transaction_hash, ch.log_index))
            .set("ticker", &ticker)
            .set("token", &ch.token)
            .set("kind", kind)
            .set("old_multiplier", nz(&ch.old_multiplier))
            .set("new_multiplier", if ch.cancelled { nz(&ch.old_multiplier) } else { nz(&ch.new_multiplier) })
            .set("effective_at", ch.effective_at as i64)
            .set("block_number", ch.announced_block)
            .set("timestamp", ch.announced_timestamp as i64)
            .set("tx_hash", &ch.transaction_hash)
            .set("log_index", ch.log_index)
            .set("announcement_id", &ch.announcement_id);
    }
    // Beryl-era instant multiplier updates (no UIMultiplierUpdated companion).
    for ev in b20.events.iter().filter(|e| e.event_type == "multiplier_updated") {
        let mirrored = b20.multiplier_changes.iter().any(|c| c.transaction_hash == ev.transaction_hash && c.token == ev.token);
        if mirrored {
            continue;
        }
        let Some(ticker) = stocks::resolve_ticker(&ev.token, &[], &stock_registry) else { continue };
        tables
            .create_row("stock_corporate_actions", format!("{}:{}", ev.transaction_hash, ev.log_index))
            .set("ticker", &ticker)
            .set("token", &ev.token)
            .set("kind", "instant_legacy")
            .set("old_multiplier", "0")
            .set("new_multiplier", nz(&ev.new_value))
            .set("effective_at", ev.effective_at as i64)
            .set("block_number", ev.block_number)
            .set("timestamp", ev.timestamp as i64)
            .set("tx_hash", &ev.transaction_hash)
            .set("log_index", ev.log_index)
            .set("announcement_id", &ev.announcement_id);
    }

    // -- v1.0.0: Voter gauges and votes -------------------------------------
    for g in &gauges.events {
        match g.event_type.as_str() {
            "gauge_created" => {
                tables
                    .create_row("gauges", &g.gauge)
                    .set("gauge", &g.gauge)
                    .set("pool", &g.pool)
                    .set("pool_factory", &g.pool_factory)
                    .set("bribe_voting_reward", &g.bribe_voting_reward)
                    .set("fee_voting_reward", &g.fee_voting_reward)
                    .set("creator", &g.creator)
                    .set("created_block", g.block_number)
                    .set("created_timestamp", g.timestamp as i64);
            }
            "gauge_killed" | "gauge_revived" => {
                // Append-only so ClickHouse (whole-row replace) stays correct.
                tables
                    .create_row("gauge_status_changes", format!("{}:{}", g.transaction_hash, g.log_index))
                    .set("gauge", &g.gauge)
                    .set("alive", g.event_type == "gauge_revived")
                    .set("block_number", g.block_number)
                    .set("timestamp", g.timestamp as i64)
                    .set("tx_hash", &g.transaction_hash);
            }
            _ => {
                tables
                    .create_row("gauge_votes", format!("{}:{}", g.transaction_hash, g.log_index))
                    .set("tx_hash", &g.transaction_hash)
                    .set("log_index", g.log_index)
                    .set("block_number", g.block_number)
                    .set("timestamp", g.timestamp as i64)
                    .set("action", if g.event_type == "voted" { "vote" } else { "abstain" })
                    .set("voter", &g.voter)
                    .set("pool", &g.pool)
                    .set("token_id", nz(&g.token_id))
                    .set("weight", nz(&g.weight))
                    .set("total_weight", nz(&g.total_weight));
            }
        }
    }

    Ok(tables.to_database_changes())
}

/// Empty strings break NUMERIC columns in the SQL sink; send 0 instead.
fn nz(v: &str) -> &str {
    if v.is_empty() {
        "0"
    } else {
        v
    }
}
