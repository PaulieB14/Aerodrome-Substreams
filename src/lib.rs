//! Aerodrome Finance Substreams for Base
//!
//! High-performance substreams for tracking Aerodrome DEX events:
//! - Swaps with OHLCV candles
//! - Liquidity events (Mint/Burn)
//! - SQL sink support (PostgreSQL/ClickHouse)

mod abi;
mod pb;
mod stores;

pub use stores::{store_pool_stats, store_swap_volumes, store_unique_traders};

use abi::pool::events::{Burn, Mint, Swap, Sync};
use pb::aerodrome::{
    AerodromeLiquidity, AerodromeSwap, LiquidityEvents, SwapEvents, SyncEvent, SyncEvents,
};
use substreams::Hex;
use substreams_database_change::pb::database::DatabaseChanges;
use substreams_database_change::tables::Tables;
use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::Event;

/// Extract Aerodrome swap events from blocks
#[substreams::handlers::map]
pub fn map_swaps(blk: eth::Block) -> Result<SwapEvents, substreams::errors::Error> {
    let timestamp = blk
        .header
        .as_ref()
        .and_then(|h| h.timestamp.as_ref())
        .map(|t| t.seconds)
        .unwrap_or(0);

    let swaps: Vec<AerodromeSwap> = blk
        .receipts()
        .flat_map(|receipt| {
            let tx_hash = Hex(&receipt.transaction.hash).to_string();
            receipt
                .receipt
                .logs
                .iter()
                .filter_map(move |log| {
                    let swap = Swap::match_and_decode(log)?;
                    Some(AerodromeSwap {
                        block_number: blk.number,
                        transaction_hash: tx_hash.clone(),
                        log_index: log.index as u64,
                        pool_address: Hex(&log.address).to_string(),
                        sender: Hex(&swap.sender).to_string(),
                        recipient: Hex(&swap.to).to_string(),
                        amount0_in: swap.amount0_in.to_string(),
                        amount1_in: swap.amount1_in.to_string(),
                        amount0_out: swap.amount0_out.to_string(),
                        amount1_out: swap.amount1_out.to_string(),
                        timestamp: timestamp as u64,
                    })
                })
        })
        .collect();

    let swap_count = swaps.len() as u32;
    let total_volume = swaps.iter().fold(0u64, |acc, s| {
        acc.saturating_add(
            s.amount0_in.parse::<u64>().unwrap_or(0)
                + s.amount1_in.parse::<u64>().unwrap_or(0),
        )
    });

    Ok(SwapEvents {
        swaps,
        swap_count,
        total_volume,
    })
}

/// Extract liquidity events (Mint/Burn)
#[substreams::handlers::map]
pub fn map_liquidity(blk: eth::Block) -> Result<LiquidityEvents, substreams::errors::Error> {
    let timestamp = blk
        .header
        .as_ref()
        .and_then(|h| h.timestamp.as_ref())
        .map(|t| t.seconds)
        .unwrap_or(0);

    let mut events = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();

        for log in &receipt.receipt.logs {
            // Mint events (add liquidity)
            if let Some(mint) = Mint::match_and_decode(log) {
                events.push(AerodromeLiquidity {
                    block_number: blk.number,
                    transaction_hash: tx_hash.clone(),
                    log_index: log.index as u64,
                    pool_address: Hex(&log.address).to_string(),
                    sender: Hex(&mint.sender).to_string(),
                    recipient: Hex(&mint.to).to_string(),
                    amount0: mint.amount0.to_string(),
                    amount1: mint.amount1.to_string(),
                    action: "mint".to_string(),
                    timestamp: timestamp as u64,
                });
            }

            // Burn events (remove liquidity)
            if let Some(burn) = Burn::match_and_decode(log) {
                events.push(AerodromeLiquidity {
                    block_number: blk.number,
                    transaction_hash: tx_hash.clone(),
                    log_index: log.index as u64,
                    pool_address: Hex(&log.address).to_string(),
                    sender: Hex(&burn.sender).to_string(),
                    recipient: Hex(&burn.to).to_string(),
                    amount0: burn.amount0.to_string(),
                    amount1: burn.amount1.to_string(),
                    action: "burn".to_string(),
                    timestamp: timestamp as u64,
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

/// Extract Sync events (reserve updates)
#[substreams::handlers::map]
pub fn map_syncs(blk: eth::Block) -> Result<SyncEvents, substreams::errors::Error> {
    let timestamp = blk
        .header
        .as_ref()
        .and_then(|h| h.timestamp.as_ref())
        .map(|t| t.seconds)
        .unwrap_or(0);

    let syncs: Vec<SyncEvent> = blk
        .receipts()
        .flat_map(|receipt| {
            receipt.receipt.logs.iter().filter_map(move |log| {
                let sync = Sync::match_and_decode(log)?;
                Some(SyncEvent {
                    block_number: blk.number,
                    pool_address: Hex(&log.address).to_string(),
                    reserve0: sync.reserve0.to_string(),
                    reserve1: sync.reserve1.to_string(),
                    timestamp: timestamp as u64,
                })
            })
        })
        .collect();

    let event_count = syncs.len() as u32;
    Ok(SyncEvents {
        events: syncs,
        event_count,
    })
}

/// Database sink output for swap events
///
/// Produces CDC records for:
/// - Individual swaps (create_row)
/// - Swap prices for candle building via SQL materialized views
///
/// Note: Aggregations (candles, stats) are computed via SQL materialized views
/// in schema.sql and schema.clickhouse.sql
#[substreams::handlers::map]
pub fn db_out(swaps: SwapEvents) -> Result<DatabaseChanges, substreams::errors::Error> {
    let mut tables = Tables::new();

    for swap in &swaps.swaps {
        // Create unique swap ID
        let swap_id = format!("{}:{}", swap.transaction_hash, swap.log_index);

        // Calculate total amounts for price computation
        let amount_in: u64 = swap.amount0_in.parse().unwrap_or(0)
            + swap.amount1_in.parse().unwrap_or(0);
        let amount_out: u64 = swap.amount0_out.parse().unwrap_or(0)
            + swap.amount1_out.parse().unwrap_or(0);

        // Calculate price ratio (scaled by 1M for precision)
        let price_ratio = if amount_in > 0 {
            ((amount_out as f64 / amount_in as f64) * 1_000_000.0) as i64
        } else {
            0
        };

        // Insert individual swap with computed fields
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
            .set("amount_in_total", amount_in)
            .set("amount_out_total", amount_out)
            .set("price_ratio", price_ratio);
    }

    Ok(tables.to_database_changes())
}
