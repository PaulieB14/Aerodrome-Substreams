//! Aerodrome Finance Substreams for Base.
//!
//! Covers both Aerodrome venues:
//!
//! * **v2** -- the volatile/stable AMM (`Swap`, `Mint`, `Burn`, `Sync`)
//! * **Slipstream** -- concentrated liquidity (`Swap`, `Mint`, `Burn`, `Collect`),
//!   which is where Coinbase B20 tokenized stocks trade
//!
//! Pools are discovered from factory `PoolCreated` events rather than from a
//! hardcoded list, and any pool with a B20 token on one side is flagged as a
//! tokenized-stock market. New tickers are picked up automatically.

mod abi;
pub mod b20;
mod pb;
mod b20_events;
mod pools;
mod stores;

pub use b20_events::map_b20_events;
pub use pools::map_pools;
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
use substreams::store::{StoreGet, StoreGetProto};
use substreams::Hex;
use substreams_database_change::pb::database::DatabaseChanges;
use substreams_database_change::tables::Tables;
use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::Event;

fn block_timestamp(blk: &eth::Block) -> u64 {
    blk.header
        .as_ref()
        .and_then(|h| h.timestamp.as_ref())
        .map(|t| t.seconds as u64)
        .unwrap_or(0)
}

fn lookup_pool(store: &StoreGetProto<Pool>, pool_address: &str) -> Option<Pool> {
    store.get_last(stores::pool_key(pool_address))
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

/// Extracts v2 swaps, enriched with pool metadata and the tokenized-stock flag.
///
/// v2 swaps are emitted whether or not the pool is in the registry: Aerodrome's
/// `Swap` signature reorders Uniswap V2's parameters, so its topic is effectively
/// unique to Aerodrome pools on Base.
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
            let pool = lookup_pool(&pool_store, &pool_address);

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
                token0: pool.as_ref().map(|p| p.token0.clone()).unwrap_or_default(),
                token1: pool.as_ref().map(|p| p.token1.clone()).unwrap_or_default(),
                is_b20: pool.as_ref().map(|p| p.is_b20).unwrap_or(false),
                b20_token: pool
                    .as_ref()
                    .map(|p| p.b20_token.clone())
                    .unwrap_or_default(),
                pool_type: pool.as_ref().map(|p| p.pool_type.clone()).unwrap_or_default(),
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
        })
        .to_u64();

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
                let pool = lookup_pool(&pool_store, &pool_address);
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
                    token0: pool.as_ref().map(|p| p.token0.clone()).unwrap_or_default(),
                    token1: pool.as_ref().map(|p| p.token1.clone()).unwrap_or_default(),
                    is_b20: pool.as_ref().map(|p| p.is_b20).unwrap_or(false),
                    b20_token: pool
                        .as_ref()
                        .map(|p| p.b20_token.clone())
                        .unwrap_or_default(),
                });
                continue;
            }

            if let Some(burn) = Burn::match_and_decode(log) {
                let pool = lookup_pool(&pool_store, &pool_address);
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
                    token0: pool.as_ref().map(|p| p.token0.clone()).unwrap_or_default(),
                    token1: pool.as_ref().map(|p| p.token1.clone()).unwrap_or_default(),
                    is_b20: pool.as_ref().map(|p| p.is_b20).unwrap_or(false),
                    b20_token: pool
                        .as_ref()
                        .map(|p| p.b20_token.clone())
                        .unwrap_or_default(),
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
            let pool = lookup_pool(&pool_store, &pool_address);

            syncs.push(SyncEvent {
                block_number: blk.number,
                pool_address,
                reserve0: sync.reserve0.to_string(),
                reserve1: sync.reserve1.to_string(),
                timestamp,
                token0: pool.as_ref().map(|p| p.token0.clone()).unwrap_or_default(),
                token1: pool.as_ref().map(|p| p.token1.clone()).unwrap_or_default(),
                is_b20: pool.as_ref().map(|p| p.is_b20).unwrap_or(false),
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
            .set("token1_symbol", &pool.token1_symbol);
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

        tables
            .create_row("aerodrome_cl_liquidity", &event_id)
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
            .set("amount", &event.amount)
            .set("amount0", &event.amount0)
            .set("amount1", &event.amount1)
            .set("token0", &event.token0)
            .set("token1", &event.token1)
            .set("is_b20", event.is_b20)
            .set("b20_token", &event.b20_token);
    }

    for event in &b20.events {
        let event_id = format!("{}:{}", event.transaction_hash, event.log_index);

        tables
            .create_row("b20_events", &event_id)
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
            .set("amount", &event.amount)
            .set("old_value", &event.old_value)
            .set("new_value", &event.new_value)
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

        tables
            .create_row("b20_multiplier_schedule", &change_id)
            .set("token", &change.token)
            .set("old_multiplier", &change.old_multiplier)
            .set("new_multiplier", &change.new_multiplier)
            .set("effective_at", change.effective_at as i64)
            .set("announced_block", change.announced_block)
            .set("announced_timestamp", change.announced_timestamp as i64)
            .set("tx_hash", &change.transaction_hash)
            .set("log_index", change.log_index)
            .set("scheduled", change.scheduled)
            .set("cancelled", change.cancelled)
            .set("announcement_id", &change.announcement_id);
    }

    Ok(tables.to_database_changes())
}
