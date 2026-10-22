//! Base B20 token events.
//!
//! Indexes the B20 surface for every B20 token on Base, which for Aerodrome
//! means the tokenized stocks trading in its Slipstream pools. Three things here
//! are easy to get wrong and matter downstream:
//!
//! * **Metadata is mutable.** `AAPLc` was created as `AAPL` and renamed later via
//!   `updateSymbol`, so a symbol captured once at pool creation goes stale.
//!   `NameUpdated` / `SymbolUpdated` keep the registry current.
//! * **The multiplier is lazy.** A scheduled update names a future `effectiveAt`
//!   and nothing is emitted when it actually flips, so the schedule has to be
//!   recorded and applied by timestamp rather than by block.
//! * **Announcement brackets carry intent.** The spec asks indexers to treat
//!   effects emitted outside an `Announcement` / `EndAnnouncement` bracket as
//!   emergency overrides, so each event records whether it was announced.
//!
//! Both multiplier generations are decoded: Beryl (live) emits only the
//! deprecated `MultiplierUpdated`, while Cobalt adds the ERC-8056
//! `UIMultiplierUpdated` and `UIMultiplierUpdateCancelled`.

use crate::abi::{b20 as b20_abi, b20_factory};
use crate::b20;
use crate::pb::aerodrome::{B20Event, B20Events, B20MultiplierChange};
use std::collections::HashMap;
use substreams::Hex;
use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::Event;

fn pausable_feature_name(feature: u64) -> &'static str {
    match feature {
        0 => "TRANSFER",
        1 => "MINT",
        2 => "BURN",
        3 => "SEIZE",
        _ => "UNKNOWN",
    }
}

fn features_to_string(features: &[substreams::scalar::BigInt]) -> String {
    features
        .iter()
        .map(|f| pausable_feature_name(f.to_u64()))
        .collect::<Vec<_>>()
        .join(",")
}

/// Decodes every B20 log in the block, plus `B20Created` from the factory precompile.
#[substreams::handlers::map]
pub fn map_b20_events(blk: eth::Block) -> Result<B20Events, substreams::errors::Error> {
    let timestamp = blk.header.timestamp.seconds as u64;

    let mut events: Vec<B20Event> = Vec::new();
    let mut multiplier_changes: Vec<B20MultiplierChange> = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();
        // Announcement brackets never nest, but several tokens can be announced
        // within one transaction, so the open bracket is tracked per token.
        let mut open_announcements: HashMap<Vec<u8>, String> = HashMap::new();

        for log in &receipt.receipt.logs {
            // `B20Created` comes from the factory precompile, not from the token.
            if log.address == b20::B20_FACTORY {
                if let Some(ev) = b20_factory::events::B20Created::match_and_decode(log) {
                    events.push(B20Event {
                        block_number: blk.number,
                        transaction_hash: tx_hash.clone(),
                        log_index: log.index as u64,
                        token: Hex(&ev.token).to_string(),
                        variant: b20::variant_name(ev.variant.to_u64() as u8).to_string(),
                        event_type: "b20_created".to_string(),
                        timestamp,
                        actor: Hex(&receipt.transaction.from).to_string(),
                        text: ev.symbol.clone(),
                        key: ev.name.clone(),
                        new_value: ev.decimals.to_string(),
                        ..Default::default()
                    });
                }
                continue;
            }

            if !b20::is_b20(&log.address) {
                continue;
            }

            let token = Hex(&log.address).to_string();
            let variant = b20::variant_of(&log.address).to_string();
            let announcement_id = open_announcements
                .get(&log.address)
                .cloned()
                .unwrap_or_default();
            let in_announcement = !announcement_id.is_empty();

            let base = B20Event {
                block_number: blk.number,
                transaction_hash: tx_hash.clone(),
                log_index: log.index as u64,
                token: token.clone(),
                variant: variant.clone(),
                timestamp,
                announcement_id: announcement_id.clone(),
                in_announcement,
                ..Default::default()
            };

            if let Some(ev) = b20_abi::events::Announcement::match_and_decode(log) {
                open_announcements.insert(log.address.clone(), ev.id.clone());
                events.push(B20Event {
                    event_type: "announcement".to_string(),
                    actor: Hex(&ev.caller).to_string(),
                    announcement_id: ev.id.clone(),
                    in_announcement: true,
                    text: ev.description,
                    uri: ev.uri,
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::EndAnnouncement::match_and_decode(log) {
                open_announcements.remove(&log.address);
                events.push(B20Event {
                    event_type: "end_announcement".to_string(),
                    announcement_id: ev.id,
                    in_announcement: true,
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::UiMultiplierUpdated::match_and_decode(log) {
                let effective_at = ev.effective_at_timestamp.to_u64();
                multiplier_changes.push(B20MultiplierChange {
                    token: token.clone(),
                    old_multiplier: ev.old_multiplier.to_string(),
                    new_multiplier: ev.new_multiplier.to_string(),
                    effective_at,
                    announced_block: blk.number,
                    announced_timestamp: timestamp,
                    transaction_hash: tx_hash.clone(),
                    log_index: log.index as u64,
                    // An instant override stamps effectiveAt at the current block;
                    // anything later was scheduled ahead of time.
                    scheduled: effective_at > timestamp,
                    cancelled: false,
                    announcement_id: announcement_id.clone(),
                });
                events.push(B20Event {
                    event_type: "ui_multiplier_updated".to_string(),
                    old_value: ev.old_multiplier.to_string(),
                    new_value: ev.new_multiplier.to_string(),
                    effective_at,
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::MultiplierUpdated::match_and_decode(log) {
                // Deprecated instant setter. Cobalt emits it alongside
                // UIMultiplierUpdated; on Beryl it is the only multiplier event,
                // so the timeline is built from it when it stands alone.
                events.push(B20Event {
                    event_type: "multiplier_updated".to_string(),
                    new_value: ev.multiplier.to_string(),
                    effective_at: timestamp,
                    ..base
                });
            } else if let Some(ev) =
                b20_abi::events::UiMultiplierUpdateCancelled::match_and_decode(log)
            {
                multiplier_changes.push(B20MultiplierChange {
                    token: token.clone(),
                    old_multiplier: ev.cancelled_multiplier.to_string(),
                    new_multiplier: String::new(),
                    effective_at: ev.cancelled_effective_at.to_u64(),
                    announced_block: blk.number,
                    announced_timestamp: timestamp,
                    transaction_hash: tx_hash.clone(),
                    log_index: log.index as u64,
                    scheduled: false,
                    cancelled: true,
                    announcement_id: announcement_id.clone(),
                });
                events.push(B20Event {
                    event_type: "ui_multiplier_update_cancelled".to_string(),
                    old_value: ev.cancelled_multiplier.to_string(),
                    effective_at: ev.cancelled_effective_at.to_u64(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::SymbolUpdated::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "symbol_updated".to_string(),
                    actor: Hex(&ev.updater).to_string(),
                    text: ev.new_symbol,
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::NameUpdated::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "name_updated".to_string(),
                    actor: Hex(&ev.updater).to_string(),
                    text: ev.new_name,
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::ExtraMetadataUpdated::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "extra_metadata_updated".to_string(),
                    key: ev.key,
                    text: ev.value,
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::Paused::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "paused".to_string(),
                    actor: Hex(&ev.updater).to_string(),
                    pause_features: features_to_string(&ev.features),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::Unpaused::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "unpaused".to_string(),
                    actor: Hex(&ev.updater).to_string(),
                    pause_features: features_to_string(&ev.features),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::Seized::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "seized".to_string(),
                    actor: Hex(&ev.caller).to_string(),
                    account: Hex(&ev.from).to_string(),
                    recipient: Hex(&ev.to).to_string(),
                    amount: ev.amount.to_string(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::BurnedBlocked::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "burned_blocked".to_string(),
                    actor: Hex(&ev.caller).to_string(),
                    account: Hex(&ev.from).to_string(),
                    amount: ev.amount.to_string(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::PolicyUpdated::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "policy_updated".to_string(),
                    key: Hex(&ev.policy_scope).to_string(),
                    old_value: ev.old_policy_id.to_string(),
                    new_value: ev.new_policy_id.to_string(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::SupplyCapUpdated::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "supply_cap_updated".to_string(),
                    actor: Hex(&ev.updater).to_string(),
                    old_value: ev.old_supply_cap.to_string(),
                    new_value: ev.new_supply_cap.to_string(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::RoleGranted::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "role_granted".to_string(),
                    actor: Hex(&ev.sender).to_string(),
                    account: Hex(&ev.account).to_string(),
                    key: Hex(&ev.role).to_string(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::RoleRevoked::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "role_revoked".to_string(),
                    actor: Hex(&ev.sender).to_string(),
                    account: Hex(&ev.account).to_string(),
                    key: Hex(&ev.role).to_string(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::LastAdminRenounced::match_and_decode(log) {
                events.push(B20Event {
                    event_type: "last_admin_renounced".to_string(),
                    account: Hex(&ev.previous_admin).to_string(),
                    ..base
                });
            } else if let Some(ev) = b20_abi::events::Memo::match_and_decode(log) {
                // A memo always sits immediately after the operation it annotates,
                // so consumers join on (transaction_hash, log_index - 1).
                events.push(B20Event {
                    event_type: "memo".to_string(),
                    actor: Hex(&ev.caller).to_string(),
                    text: Hex(&ev.memo).to_string(),
                    ..base
                });
            } else if b20_abi::events::ContractUriUpdated::match_and_decode(log).is_some() {
                events.push(B20Event {
                    event_type: "contract_uri_updated".to_string(),
                    ..base
                });
            }
        }
    }

    let count = events.len() as u32;
    Ok(B20Events {
        events,
        count,
        multiplier_changes,
    })
}
