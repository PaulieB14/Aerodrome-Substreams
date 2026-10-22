//! Store modules for Aerodrome Substreams
//!
//! Provides persistent state tracking across blocks for:
//! - Cumulative swap volumes by pool
//! - Unique trader (wallet) tracking
//! - Pool statistics
//! - Daily/hourly aggregations

use crate::pb::aerodrome::{ClSwaps, Pool, Pools, SwapEvents};
use std::str::FromStr;
use substreams::scalar::BigInt;
use substreams::store::{
    StoreAdd, StoreAddBigInt, StoreNew, StoreSet, StoreSetIfNotExists, StoreSetIfNotExistsString,
    StoreSetProto,
};

/// Registry key for a pool. Kept in one place so writers and readers cannot drift.
pub fn pool_key(address: &str) -> String {
    format!("pool:{}", address)
}

/// Persists every pool discovered from a factory `PoolCreated` event.
///
/// Downstream maps read this in `get` mode to attach token metadata and the
/// tokenized-stock flag to swaps, and -- for Slipstream -- to tell an Aerodrome
/// pool apart from any other Uniswap V3-style pool on Base, which emit a
/// byte-identical `Swap` topic.
#[substreams::handlers::store]
pub fn store_pools(pools: Pools, store: StoreSetProto<Pool>) {
    for pool in pools.pools {
        store.set(0, pool_key(&pool.address), &pool);
    }
}

/// Parses a uint256 amount without truncating.
///
/// Swap amounts routinely exceed `u64::MAX` (any trade over ~18.4 tokens of an
/// 18-decimal asset), so parsing them as `u64` silently drops them to zero.
fn amount(value: &str) -> BigInt {
    BigInt::from_str(value).unwrap_or_else(|_| BigInt::zero())
}

/// Sums the input side of a v2 swap.
fn swap_volume_in(amount0_in: &str, amount1_in: &str) -> BigInt {
    amount(amount0_in) + amount(amount1_in)
}

/// Store handler for tracking cumulative swap volumes by pool
///
/// Key formats:
/// - `pool:{address}:volume` - Total volume for pool
/// - `pool:{address}:count` - Total swap count for pool
/// - `daily:{date}:volume` - Daily volume
/// - `daily:{date}:count` - Daily swap count
/// - `hourly:{hour}:volume` - Hourly volume
/// - `total:volume` - Protocol-wide total volume
/// - `total:swaps` - Protocol-wide swap count
#[substreams::handlers::store]
pub fn store_swap_volumes(swaps: SwapEvents, store: StoreAddBigInt) {
    for swap in &swaps.swaps {
        let amount_in = swap_volume_in(&swap.amount0_in, &swap.amount1_in);

        if amount_in == BigInt::zero() {
            continue;
        }

        // Store volume by pool
        let pool_volume_key = format!("pool:{}:volume", swap.pool_address);
        store.add(0, &pool_volume_key, &amount_in);

        // Store swap count by pool
        let pool_count_key = format!("pool:{}:count", swap.pool_address);
        store.add(0, &pool_count_key, &BigInt::from(1u64));

        // Store total protocol volume
        store.add(0, "total:volume", &amount_in);

        // Store total swap count
        store.add(0, "total:swaps", &BigInt::from(1u64));

        // Store daily volume
        let date = format_date(swap.timestamp);
        let daily_volume_key = format!("daily:{}:volume", date);
        store.add(0, &daily_volume_key, &amount_in);

        let daily_count_key = format!("daily:{}:count", date);
        store.add(0, &daily_count_key, &BigInt::from(1u64));

        // Store hourly volume
        let hour = format_hour(swap.timestamp);
        let hourly_volume_key = format!("hourly:{}:volume", hour);
        store.add(0, &hourly_volume_key, &amount_in);

        let hourly_count_key = format!("hourly:{}:count", hour);
        store.add(0, &hourly_count_key, &BigInt::from(1u64));
    }
}

/// Store handler for tracking unique traders (wallets)
///
/// Key formats:
/// - `trader:{address}` - First seen timestamp for wallet
/// - `daily:{date}:trader:{address}` - Daily unique trader tracking
/// - `pool:{pool}:trader:{address}` - Per-pool unique trader tracking
#[substreams::handlers::store]
pub fn store_unique_traders(swaps: SwapEvents, store: StoreSetIfNotExistsString) {
    for swap in &swaps.swaps {
        if swap.sender.is_empty() {
            continue;
        }

        // Track unique trader with first seen timestamp
        let trader_key = format!("trader:{}", swap.sender);
        let value = format!("{}:{}", swap.block_number, swap.timestamp);
        store.set_if_not_exists(0, &trader_key, &value);

        // Track daily unique traders
        let date = format_date(swap.timestamp);
        let daily_trader_key = format!("daily:{}:trader:{}", date, swap.sender);
        store.set_if_not_exists(0, &daily_trader_key, &swap.block_number.to_string());

        // Track traders per pool
        let pool_trader_key = format!("pool:{}:trader:{}", swap.pool_address, swap.sender);
        store.set_if_not_exists(0, &pool_trader_key, &swap.block_number.to_string());
    }
}

/// Store handler for tracking pool statistics
///
/// Key formats:
/// - `pool:{address}:trade_count` - Total trades for pool
/// - `pool:{address}:unique_pairs` - Tracks trading activity
#[substreams::handlers::store]
pub fn store_pool_stats(swaps: SwapEvents, store: StoreAddBigInt) {
    for swap in &swaps.swaps {
        if swap_volume_in(&swap.amount0_in, &swap.amount1_in) == BigInt::zero() {
            continue;
        }

        // Track pool trade count
        let count_key = format!("pool:{}:trade_count", swap.pool_address);
        store.add(0, &count_key, &BigInt::from(1u64));
    }
}

/// Cumulative Slipstream volume, split out so B20 markets -- the tokenized
/// stocks among them -- can be totalled without scanning every swap row.
///
/// Key formats:
/// - `cl:pool:{address}:volume` / `:count`
/// - `b20:{token}:volume` / `:count`   (per B20 token)
/// - `b20:total:volume` / `:count`     (all B20 tokens)
#[substreams::handlers::store]
pub fn store_cl_volumes(swaps: ClSwaps, store: StoreAddBigInt) {
    for swap in &swaps.swaps {
        // Slipstream reports signed pool deltas; size is the magnitude traded.
        let volume = amount(&swap.amount0).absolute();

        if volume == BigInt::zero() {
            continue;
        }

        store.add(0, format!("cl:pool:{}:volume", swap.pool_address), &volume);
        store.add(0, format!("cl:pool:{}:count", swap.pool_address), &BigInt::from(1u64));
        // v1.0.0: token1-side volume too (v0.3.x only tracked amount0).
        let volume1 = amount(&swap.amount1).absolute();
        store.add(0, format!("cl:pool:{}:volume1", swap.pool_address), &volume1);

        if swap.is_b20 {
            store.add(0, format!("b20:{}:volume", swap.b20_token), &volume);
            store.add(0, format!("b20:{}:count", swap.b20_token), &BigInt::from(1u64));
            store.add(0, "b20:total:volume", &volume);
            store.add(0, "b20:total:count", &BigInt::from(1u64));
        }
    }
}

/// Format Unix timestamp to YYYY-MM-DD date string
pub(crate) fn format_date(timestamp: u64) -> String {
    let days = timestamp / 86400;
    let mut year = 1970u64;
    let mut remaining_days = days;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [u64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u64;
    for days_in_month in days_in_months.iter() {
        if remaining_days < *days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }

    let day = remaining_days + 1;
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Format Unix timestamp to YYYY-MM-DD-HH hour string
fn format_hour(timestamp: u64) -> String {
    let date = format_date(timestamp);
    let hour = (timestamp % 86400) / 3600;
    format!("{}-{:02}", date, hour)
}

/// Check if a year is a leap year
#[inline]
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_date() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1705276800), "2024-01-15");
        assert_eq!(format_date(1582934400), "2020-02-29"); // Leap year
    }

    #[test]
    fn test_format_hour() {
        assert_eq!(format_hour(0), "1970-01-01-00");
        assert_eq!(format_hour(3600), "1970-01-01-01");
        assert_eq!(format_hour(86399), "1970-01-01-23");
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2020));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(2100));
        assert!(!is_leap_year(2023));
    }
}
