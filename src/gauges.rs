//! Aerodrome Voter: gauge lifecycle and veAERO votes.
//!
//! Voter address and event signatures come from the official
//! aerodrome-finance/contracts repository (commit 1ba30815bba620f7e9faa34769ffd00c214c9b82):
//! README.md deployment table (`Voter 0x16613524e02ad97eDfeF371bC883F2F5d6C480A5`) and
//! contracts/interfaces/IVoter.sol. Slipstream gauges are created through the same
//! Voter (see aerodrome-finance/slipstream README), so one emitter covers both.

use crate::abi::voter::events::{Abstained, GaugeCreated, GaugeKilled, GaugeRevived, Voted};
use crate::pb::aerodrome::{GaugeEvent, GaugeEvents};
use substreams::Hex;
use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::Event;

pub const VOTER: [u8; 20] = hex_literal::hex!("16613524e02ad97edfef371bc883f2f5d6c480a5");

#[substreams::handlers::map]
pub fn map_gauge_events(blk: eth::Block) -> Result<GaugeEvents, substreams::errors::Error> {
    let timestamp = blk.header.timestamp.seconds as u64;
    let mut events = Vec::new();
    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();
        for log in &receipt.receipt.logs {
            if log.address != VOTER {
                continue;
            }
            let base = GaugeEvent {
                block_number: blk.number,
                transaction_hash: tx_hash.clone(),
                log_index: log.index as u64,
                timestamp,
                ..Default::default()
            };
            if let Some(ev) = GaugeCreated::match_and_decode(log) {
                events.push(GaugeEvent {
                    event_type: "gauge_created".into(),
                    gauge: Hex(&ev.gauge).to_string(),
                    pool: Hex(&ev.pool).to_string(),
                    pool_factory: Hex(&ev.pool_factory).to_string(),
                    bribe_voting_reward: Hex(&ev.bribe_voting_reward).to_string(),
                    fee_voting_reward: Hex(&ev.fee_voting_reward).to_string(),
                    creator: Hex(&ev.creator).to_string(),
                    ..base
                });
            } else if let Some(ev) = GaugeKilled::match_and_decode(log) {
                events.push(GaugeEvent { event_type: "gauge_killed".into(), gauge: Hex(&ev.gauge).to_string(), ..base });
            } else if let Some(ev) = GaugeRevived::match_and_decode(log) {
                events.push(GaugeEvent { event_type: "gauge_revived".into(), gauge: Hex(&ev.gauge).to_string(), ..base });
            } else if let Some(ev) = Voted::match_and_decode(log) {
                events.push(GaugeEvent {
                    event_type: "voted".into(),
                    pool: Hex(&ev.pool).to_string(),
                    voter: Hex(&ev.voter).to_string(),
                    token_id: ev.token_id.to_string(),
                    weight: ev.weight.to_string(),
                    total_weight: ev.total_weight.to_string(),
                    ..base
                });
            } else if let Some(ev) = Abstained::match_and_decode(log) {
                events.push(GaugeEvent {
                    event_type: "abstained".into(),
                    pool: Hex(&ev.pool).to_string(),
                    voter: Hex(&ev.voter).to_string(),
                    token_id: ev.token_id.to_string(),
                    weight: ev.weight.to_string(),
                    total_weight: ev.total_weight.to_string(),
                    ..base
                });
            }
        }
    }
    Ok(GaugeEvents { events })
}
