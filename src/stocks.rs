//! Coinbase tokenized stocks on Aerodrome (v1.0.0).
//!
//! Sources (all official):
//! * Aerodrome, "Coinbase Tokenized Stocks Launch on Aerodrome" (2026-08-24),
//!   https://aero.xyz/articles/coinbase-tokenized-stocks-launch-on-aerodrome/ -- stocks are
//!   Coinbase-issued B20 tokens with day-one liquidity on Aerodrome.
//! * Token list: Coinbase Tokenized Stocks API (`stock_list.rs`), cross-checked with
//!   https://www.base.org/stocks and https://docs.base.org/specifications/b20/tokenized-stocks-on-base.
//! * Multiplier semantics: https://docs.base.org/specifications/b20/concepts/multipliers
//!   (UI amount = raw * multiplier / 1e18; a scheduled update flips at `effectiveAt`
//!   with no event; a stored 0 reads as 1e18).
//! * USD stablecoin: native USDC on Base per Circle,
//!   https://developers.circle.com/stablecoins/usdc-contract-addresses (6 decimals).
//!   Other quotes get no USD notional rather than a guessed peg.
//!
//! Registry: the static list, plus every `B20Created` whose transaction also
//! carries a log from the Coinbase token factory (auto-detection of new listings),
//! plus the `extra_stock_tokens` module param (`0xaddr:TICKER,0xaddr:TICKER`).

use crate::pb::aerodrome::{B20Events, ClSwaps, Pool, StockSwap, StockSwaps, SwapEvents};
use crate::stock_list::COINBASE_STOCKS;
use std::str::FromStr;
use substreams::scalar::{BigDecimal, BigInt};
use substreams::store::{
    StoreAdd, StoreAddBigDecimal, StoreGet, StoreGetProto, StoreGetString, StoreNew, StoreSet,
    StoreSetString,
};

pub const USD_STABLES: &[(&str, &str, i32)] = &[
    ("833589fcd6edb6e08f4c7c32d4f71b54bda02913", "USDC", 6),
];

const WAD: &str = "1000000000000000000";

fn norm(addr: &str) -> String {
    addr.trim().trim_start_matches("0x").to_lowercase()
}

pub fn static_stock(token: &str) -> Option<(&'static str, u32)> {
    let t = norm(token);
    COINBASE_STOCKS
        .iter()
        .find(|(a, ..)| *a == t)
        .map(|(_, sym, _, dec, _)| (*sym, *dec))
}

/// Parses `0xaddr:TICKER,0xaddr:TICKER`.
pub fn parse_extra(params: &str) -> Vec<(String, String)> {
    params
        .split(',')
        .filter_map(|kv| {
            let (a, t) = kv.split_once(':')?;
            let a = norm(a);
            (a.len() == 40 && !t.trim().is_empty()).then(|| (a, t.trim().to_string()))
        })
        .collect()
}

/// Resolves the current ticker of a stock token, or None if it is not a stock.
pub fn resolve_ticker(token: &str, extra: &[(String, String)], reg: &StoreGetString) -> Option<String> {
    let t = norm(token);
    if t.is_empty() {
        return None;
    }
    let base = static_stock(&t)
        .map(|(s, _)| s.to_string())
        .or_else(|| extra.iter().find(|(a, _)| *a == t).map(|(_, s)| s.clone()))
        .or_else(|| reg.get_last(format!("t:{}", t)))?;
    // Metadata is mutable on B20; prefer the latest SymbolUpdated.
    Some(reg.get_last(format!("sym:{}", t)).filter(|s| !s.is_empty()).unwrap_or(base))
}

/// The B20 UI multiplier in effect at `ts` (WAD-scaled).
pub fn multiplier_at(reg: &StoreGetString, token: &str, ts: u64) -> BigInt {
    let wad = BigInt::from_str(WAD).unwrap();
    let t = norm(token);
    let Some(m) = reg.get_last(format!("m:{}", t)) else {
        return wad;
    };
    let parts: Vec<&str> = m.split('|').collect();
    if parts.len() != 3 {
        return wad;
    }
    let old = BigInt::from_str(parts[0]).unwrap_or_else(|_| BigInt::zero());
    let new = BigInt::from_str(parts[1]).unwrap_or_else(|_| BigInt::zero());
    let eff: u64 = parts[2].parse().unwrap_or(0);
    let cancelled = reg
        .get_last(format!("c:{}", t))
        .map(|c| !c.is_empty() && c == parts[2])
        .unwrap_or(false);
    let cur = if cancelled || ts < eff { old } else { new };
    if cur == BigInt::zero() {
        wad
    } else {
        cur
    }
}

/// Registry + multiplier timeline, keyed:
/// `t:<token>` ticker of an auto-detected Coinbase stock, `sym:<token>` latest
/// symbol, `m:<token>` `old|new|effectiveAt`, `c:<token>` cancelled effectiveAt.
#[substreams::handlers::store]
pub fn store_stock_registry(b20: B20Events, s: StoreSetString) {
    for ev in &b20.events {
        let ord = ev.log_index;
        let t = norm(&ev.token);
        match ev.event_type.as_str() {
            "b20_created" if ev.coinbase_issued => s.set(ord, format!("t:{}", t), &ev.text),
            "symbol_updated" => s.set(ord, format!("sym:{}", t), &ev.text),
            "ui_multiplier_updated" => {
                s.set(ord, format!("m:{}", t), &format!("{}|{}|{}", ev.old_value, ev.new_value, ev.effective_at));
                s.set(ord, format!("c:{}", t), &String::new());
            }
            "multiplier_updated" => {
                // Beryl's instant setter emits only this. On Cobalt it mirrors a
                // UIMultiplierUpdated in the same tx, which already carries the schedule.
                let mirrored = b20.events.iter().any(|o| {
                    o.event_type == "ui_multiplier_updated"
                        && o.transaction_hash == ev.transaction_hash
                        && o.token == ev.token
                });
                if !mirrored {
                    s.set(ord, format!("m:{}", t), &format!("{}|{}|0", ev.new_value, ev.new_value));
                }
            }
            "ui_multiplier_update_cancelled" => {
                s.set(ord, format!("c:{}", t), &ev.effective_at.to_string())
            }
            _ => {}
        }
    }
}

fn dec(raw: &BigInt, decimals: i32) -> BigDecimal {
    let d = if decimals < 0 { 0 } else { decimals as u32 };
    let scale = BigInt::from(10u64).pow(d);
    BigDecimal::from(raw.clone()) / BigDecimal::from(scale)
}

/// Decimal string with at most 30 significant digits and no trailing zeros.
fn s(v: BigDecimal) -> String {
    let out = v.with_prec(30).to_string();
    if out.contains('.') {
        out.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        out
    }
}

fn pool_decimals(pool: &Pool, stock_is_0: bool, quote: &str) -> (i32, i32) {
    let (sd, qd) = if stock_is_0 {
        (pool.token0_decimals, pool.token1_decimals)
    } else {
        (pool.token1_decimals, pool.token0_decimals)
    };
    let sd = if sd >= 0 { sd } else { static_stock(&pool.b20_token).map(|(_, d)| d as i32).unwrap_or(-1) };
    let qd = if qd >= 0 {
        qd
    } else {
        USD_STABLES.iter().find(|(a, ..)| *a == norm(quote)).map(|(.., d)| *d).unwrap_or(-1)
    };
    (sd, qd)
}

struct Leg<'a> {
    pool: &'a Pool,
    block: u64,
    tx: &'a str,
    log_index: u64,
    ts: u64,
    sender: &'a str,
    recipient: &'a str,
    pool_type: &'a str,
    // signed pool deltas: positive = pool received
    d0: BigInt,
    d1: BigInt,
}

fn build(leg: Leg, extra: &[(String, String)], reg: &StoreGetString) -> Option<StockSwap> {
    let pool = leg.pool;
    if pool.b20_token.is_empty() {
        return None;
    }
    let ticker = resolve_ticker(&pool.b20_token, extra, reg)?;
    let stock_is_0 = norm(&pool.token0) == norm(&pool.b20_token);
    let (stock_d, quote_d, quote) = if stock_is_0 {
        (leg.d0, leg.d1, pool.token1.clone())
    } else {
        (leg.d1, leg.d0, pool.token0.clone())
    };
    let (sdec, qdec) = pool_decimals(pool, stock_is_0, &quote);
    let side = if stock_d < BigInt::zero() { "buy" } else { "sell" };
    let stock_raw = stock_d.absolute();
    let quote_raw = quote_d.absolute();
    let stock_amt = dec(&stock_raw, sdec);
    let quote_amt = dec(&quote_raw, qdec);
    let mult = multiplier_at(reg, &pool.b20_token, leg.ts);
    let shares = stock_amt.clone() * BigDecimal::from(mult.clone()) / BigDecimal::from_str(WAD).unwrap();
    let stable = USD_STABLES.iter().find(|(a, ..)| *a == norm(&quote));
    let quote_symbol = stable
        .map(|(_, s, _)| s.to_string())
        .or_else(|| static_stock(&quote).map(|(s, _)| s.to_string()))
        .unwrap_or_else(|| if stock_is_0 { pool.token1_symbol.clone() } else { pool.token0_symbol.clone() });
    let (usd, ppt, pps) = match stable {
        Some(_) if qdec >= 0 && sdec >= 0 => {
            let zero = BigDecimal::zero();
            let ppt = if stock_amt == zero { String::new() } else { s(quote_amt.clone() / stock_amt.clone()) };
            let pps = if shares == zero { String::new() } else { s(quote_amt.clone() / shares.clone()) };
            (s(quote_amt.clone()), ppt, pps)
        }
        _ => (String::new(), String::new(), String::new()),
    };
    Some(StockSwap {
        block_number: leg.block,
        transaction_hash: leg.tx.to_string(),
        log_index: leg.log_index,
        timestamp: leg.ts,
        pool_address: pool.address.clone(),
        pool_type: leg.pool_type.to_string(),
        ticker,
        stock_token: norm(&pool.b20_token),
        quote_token: norm(&quote),
        quote_symbol,
        side: side.to_string(),
        stock_amount_raw: stock_raw.to_string(),
        quote_amount_raw: quote_raw.to_string(),
        stock_decimals: sdec,
        quote_decimals: qdec,
        stock_amount: s(stock_amt),
        quote_amount: s(quote_amt),
        multiplier: mult.to_string(),
        share_amount: s(shares),
        usd_notional: usd,
        usd_price_per_token: ppt,
        usd_price_per_share: pps,
        sender: leg.sender.to_string(),
        recipient: leg.recipient.to_string(),
    })
}

fn big(v: &str) -> BigInt {
    BigInt::from_str(v).unwrap_or_else(|_| BigInt::zero())
}

#[substreams::handlers::map]
pub fn map_stock_swaps(
    params: String,
    swaps: SwapEvents,
    cl_swaps: ClSwaps,
    pools: StoreGetProto<Pool>,
    reg: StoreGetString,
) -> Result<StockSwaps, substreams::errors::Error> {
    let extra = parse_extra(&params);
    let mut out = Vec::new();
    for sw in swaps.swaps.iter().filter(|s| s.is_b20) {
        let Some(pool) = pools.get_last(crate::stores::pool_key(&sw.pool_address)) else { continue };
        let leg = Leg {
            pool: &pool,
            block: sw.block_number,
            tx: &sw.transaction_hash,
            log_index: sw.log_index,
            ts: sw.timestamp,
            sender: &sw.sender,
            recipient: &sw.recipient,
            pool_type: &sw.pool_type,
            d0: big(&sw.amount0_in) - big(&sw.amount0_out),
            d1: big(&sw.amount1_in) - big(&sw.amount1_out),
        };
        out.extend(build(leg, &extra, &reg));
    }
    for sw in cl_swaps.swaps.iter().filter(|s| s.is_b20) {
        let Some(pool) = pools.get_last(crate::stores::pool_key(&sw.pool_address)) else { continue };
        let leg = Leg {
            pool: &pool,
            block: sw.block_number,
            tx: &sw.transaction_hash,
            log_index: sw.log_index,
            ts: sw.timestamp,
            sender: &sw.sender,
            recipient: &sw.recipient,
            pool_type: "cl",
            d0: big(&sw.amount0),
            d1: big(&sw.amount1),
        };
        out.extend(build(leg, &extra, &reg));
    }
    out.sort_by_key(|s| s.log_index);
    Ok(StockSwaps { swaps: out })
}

/// Daily per-ticker totals, keyed `<YYYY-MM-DD>|<ticker>|<metric>` with metric in
/// usd (USD-stable pairs only), tokens, shares, count, usd_count.
#[substreams::handlers::store]
pub fn store_stock_daily_volume(swaps: StockSwaps, s: StoreAddBigDecimal) {
    for sw in &swaps.swaps {
        let day = crate::stores::format_date(sw.timestamp);
        let k = |m: &str| format!("{}|{}|{}", day, sw.ticker, m);
        let ord = sw.log_index;
        s.add(ord, k("tokens"), BigDecimal::from_str(&sw.stock_amount).unwrap_or_else(|_| BigDecimal::zero()));
        s.add(ord, k("shares"), BigDecimal::from_str(&sw.share_amount).unwrap_or_else(|_| BigDecimal::zero()));
        s.add(ord, k("count"), BigDecimal::from(1u64));
        // Every metric is touched on every swap so each delta batch carries the
        // full row (ClickHouse ReplacingMergeTree replaces whole rows).
        let usd = BigDecimal::from_str(&sw.usd_notional).unwrap_or_else(|_| BigDecimal::zero());
        let usd_n = if sw.usd_notional.is_empty() { 0u64 } else { 1u64 };
        s.add(ord, k("usd"), usd);
        s.add(ord, k("usd_count"), BigDecimal::from(usd_n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_registry_has_launch_tickers() {
        for (a, t) in [
            ("0xb20000000000000000000078ee7ce2fE4908108C", "NVDAc"),
            ("0xb200000000000000000000C2e324d24d7eEcd1fb", "AAPLc"),
            ("0xb2000000000000000000002D0BA3164cc74f58B7", "GOOGLc"),
            ("0xb2000000000000000000008bC8786B856E61707C", "METAc"),
        ] {
            assert_eq!(static_stock(a).map(|x| x.0), Some(t));
        }
        assert!(static_stock("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").is_none());
    }

    #[test]
    fn parses_extra_param() {
        let v = parse_extra("0xB2000000000000000000000000000000000000AA:FOOc, bad, :x");
        assert_eq!(v, vec![("b2000000000000000000000000000000000000aa".to_string(), "FOOc".to_string())]);
    }
}
