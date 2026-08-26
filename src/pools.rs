//! Pool discovery.
//!
//! Aerodrome pools are found by decoding `PoolCreated` from the protocol's
//! factories rather than from a hardcoded pool list. That is what makes the
//! package self-maintaining: when Coinbase lists a new tokenized stock and
//! someone opens an Aerodrome pool for it, the pool shows up here on the block
//! it is created, already flagged as a stock market.

use crate::abi::{cl_factory, erc20, factory_registry, pool_factory};
use crate::b20;
use crate::pb::aerodrome::{Factories, Factory, Pool, Pools};
use substreams::store::{StoreGet, StoreGetInt64, StoreNew, StoreSet, StoreSetInt64};
use substreams::Hex;
use substreams_ethereum::pb::eth::v2 as eth;
use substreams_ethereum::rpc::RpcBatch;
use substreams_ethereum::Event;

/// Marks a token whose metadata could not be read, so that an unknown decimal
/// count is never silently treated as 0 -- which would misprice every amount.
const UNKNOWN_DECIMALS: i32 = -1;

/// Reads `decimals()` and `symbol()` for both sides of a pair.
///
/// Called once per pool at creation, which across all of Aerodrome's history is a
/// few thousand calls in total. Failures degrade to `UNKNOWN_DECIMALS` / an empty
/// symbol rather than failing the module, so the package still runs end to end
/// against a provider that exposes no RPC.
fn token_metadata(token0: &[u8], token1: &[u8]) -> (i32, i32, String, String) {
    let responses = RpcBatch::new()
        .add(erc20::functions::Decimals {}, token0.to_vec())
        .add(erc20::functions::Symbol {}, token0.to_vec())
        .add(erc20::functions::Decimals {}, token1.to_vec())
        .add(erc20::functions::Symbol {}, token1.to_vec())
        .execute();

    let Ok(responses) = responses else {
        substreams::log::info!("token metadata rpc unavailable; continuing without decimals");
        return (UNKNOWN_DECIMALS, UNKNOWN_DECIMALS, String::new(), String::new());
    };

    let responses = responses.responses;
    let decimals = |i: usize| -> i32 {
        responses
            .get(i)
            .and_then(RpcBatch::decode::<_, erc20::functions::Decimals>)
            .map(|d: substreams::scalar::BigInt| d.to_i32())
            .unwrap_or(UNKNOWN_DECIMALS)
    };
    let symbol = |i: usize| -> String {
        responses
            .get(i)
            .and_then(RpcBatch::decode::<_, erc20::functions::Symbol>)
            .unwrap_or_default()
    };

    (decimals(0), decimals(2), symbol(1), symbol(3))
}

/// Aerodrome v2 `PoolFactory` (Base, deployed at block 3,200,559).
pub const V2_POOL_FACTORY: [u8; 20] = [
    0x42, 0x0d, 0xd3, 0x81, 0xb3, 0x1a, 0xef, 0x66, 0x83, 0xdb, 0x6b, 0x90, 0x20, 0x84, 0xcb, 0x0f,
    0xfe, 0xce, 0x40, 0xda,
];

/// Slipstream `CLFactory` v1 (Base, deployed at block 13,843,704).
pub const CL_FACTORY_V1: [u8; 20] = [
    0x5e, 0x7b, 0xb1, 0x04, 0xd8, 0x4c, 0x7c, 0xb9, 0xb6, 0x82, 0xaa, 0xc2, 0xf3, 0xd5, 0x09, 0xf5,
    0xf4, 0x06, 0x80, 0x9a,
];

/// Slipstream `CLFactory` (Base, deployed at block 36,953,918), 2,184 pools.
pub const CL_FACTORY_V2: [u8; 20] = [
    0xad, 0xe6, 0x5c, 0x38, 0xcd, 0x48, 0x49, 0xad, 0xba, 0x59, 0x5a, 0x43, 0x23, 0xa8, 0xc7, 0xdd,
    0xfe, 0x89, 0x71, 0x6a,
];

/// Slipstream `CLFactory` (Base, deployed at block 44,394,724).
/// Every Coinbase tokenized-stock CL pool live at launch was created here.
pub const CL_FACTORY_V3: [u8; 20] = [
    0xf8, 0xf2, 0xeb, 0x49, 0x40, 0xcf, 0xe7, 0xd1, 0x36, 0x03, 0xdd, 0xdd, 0x87, 0xf1, 0x23, 0x82,
    0x0f, 0xc0, 0x61, 0xef,
];

/// Aerodrome's `FactoryRegistry` -- the protocol's own list of real factories.
pub const FACTORY_REGISTRY: [u8; 20] = [
    0x5c, 0x3f, 0x18, 0xf0, 0x6c, 0xc0, 0x9c, 0xa1, 0x91, 0x07, 0x67, 0xa3, 0x4a, 0x20, 0xf7, 0x71,
    0x03, 0x9e, 0x37, 0xc0,
];

/// The four factories `FactoryRegistry.poolFactories()` reported at build time.
///
/// Seeded so history indexes correctly without replaying the registry's own
/// approvals, which predate the pools they authorise. `map_factories` keeps the
/// set current from there.
const SEED_FACTORIES: [[u8; 20]; 4] =
    [V2_POOL_FACTORY, CL_FACTORY_V1, CL_FACTORY_V2, CL_FACTORY_V3];

fn factory_key(address: &[u8]) -> String {
    format!("factory:{}", Hex(address))
}

/// Tracks factories approved and removed in Aerodrome's `FactoryRegistry`.
#[substreams::handlers::map]
pub fn map_factories(blk: eth::Block) -> Result<Factories, substreams::errors::Error> {
    let mut factories: Vec<Factory> = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();

        for log in &receipt.receipt.logs {
            if log.address != FACTORY_REGISTRY {
                continue;
            }

            if let Some(ev) = factory_registry::events::Approve::match_and_decode(log) {
                substreams::log::info!("factory approved: {}", Hex(&ev.pool_factory));
                factories.push(Factory {
                    address: Hex(&ev.pool_factory).to_string(),
                    approved: true,
                    block_number: blk.number,
                    transaction_hash: tx_hash.clone(),
                });
            } else if let Some(ev) = factory_registry::events::Unapprove::match_and_decode(log) {
                factories.push(Factory {
                    address: Hex(&ev.pool_factory).to_string(),
                    approved: false,
                    block_number: blk.number,
                    transaction_hash: tx_hash.clone(),
                });
            }
        }
    }

    let count = factories.len() as u32;
    Ok(Factories { factories, count })
}

/// Stores which pool factories are currently approved. 1 = approved, 0 = removed.
#[substreams::handlers::store]
pub fn store_factories(factories: Factories, store: StoreSetInt64) {
    for factory in factories.factories {
        store.set(
            0,
            factory_key(&hex::decode(&factory.address).unwrap_or_default()),
            &if factory.approved { 1 } else { 0 },
        );
    }
}

/// True when the emitter is a pool factory this package should trust.
///
/// The seed list covers every factory approved at build time; the store covers
/// anything approved since, so a factory added after release still indexes.
fn is_known_factory(address: &[u8], store: &StoreGetInt64) -> bool {
    if SEED_FACTORIES.iter().any(|f| f == address) {
        return true;
    }
    store.get_last(factory_key(address)) == Some(1)
}

fn build(
    address: &[u8],
    token0: &[u8],
    token1: &[u8],
    pool_type: &str,
    tick_spacing: i32,
    factory: &[u8],
    blk: &eth::Block,
    tx_hash: &str,
    timestamp: u64,
) -> Pool {
    let b20_token = b20::b20_side(token0, token1);
    let (token0_decimals, token1_decimals, token0_symbol, token1_symbol) =
        token_metadata(token0, token1);
    Pool {
        address: Hex(address).to_string(),
        token0: Hex(token0).to_string(),
        token1: Hex(token1).to_string(),
        pool_type: pool_type.to_string(),
        tick_spacing,
        factory: Hex(factory).to_string(),
        is_b20: b20_token.is_some(),
        b20_token: b20_token.map(|t| Hex(t).to_string()).unwrap_or_default(),
        b20_variant: b20_token.map(b20::variant_of).unwrap_or("").to_string(),
        created_block: blk.number,
        created_timestamp: timestamp,
        created_tx: tx_hash.to_string(),
        token0_decimals,
        token1_decimals,
        token0_symbol,
        token1_symbol,
    }
}

/// Decodes `PoolCreated` from every approved Aerodrome pool factory.
///
/// The two `PoolCreated` shapes have different topics, so the venue is decided by
/// which one decodes rather than by which factory emitted it. A newly approved
/// factory of either kind is therefore handled with no code change.
#[substreams::handlers::map]
pub fn map_pools(
    blk: eth::Block,
    factory_store: StoreGetInt64,
) -> Result<Pools, substreams::errors::Error> {
    let timestamp = blk
        .header
        .as_ref()
        .and_then(|h| h.timestamp.as_ref())
        .map(|t| t.seconds as u64)
        .unwrap_or(0);

    let mut pools: Vec<Pool> = Vec::new();

    for receipt in blk.receipts() {
        let tx_hash = Hex(&receipt.transaction.hash).to_string();

        for log in &receipt.receipt.logs {
            // Only an approved factory's PoolCreated is trusted, which keeps pools
            // from unrelated forks that reuse the same event out of the registry.
            if !is_known_factory(&log.address, &factory_store) {
                continue;
            }

            if let Some(ev) = pool_factory::events::PoolCreated::match_and_decode(log) {
                let pool_type = if ev.stable { "v2_stable" } else { "v2_volatile" };
                pools.push(build(
                    &ev.pool, &ev.token0, &ev.token1, pool_type, 0, &log.address, &blk, &tx_hash,
                    timestamp,
                ));
            } else {
                if let Some(ev) = cl_factory::events::PoolCreated::match_and_decode(log) {
                    let tick_spacing = ev.tick_spacing.to_string().parse::<i32>().unwrap_or(0);
                    pools.push(build(
                        &ev.pool,
                        &ev.token0,
                        &ev.token1,
                        "cl",
                        tick_spacing,
                        &log.address,
                        &blk,
                        &tx_hash,
                        timestamp,
                    ));
                }
            }
        }
    }

    let count = pools.len() as u32;
    let b20_count = pools.iter().filter(|p| p.is_b20).count() as u32;

    if b20_count > 0 {
        substreams::log::info!("block {}: discovered {} B20 pool(s)", blk.number, b20_count);
    }

    Ok(Pools {
        pools,
        count,
        b20_count,
    })
}
