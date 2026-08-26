# Aerodrome Finance Substreams

[![Substreams](https://img.shields.io/badge/Substreams-v0.3.0-blue)](https://substreams.dev)
[![Base](https://img.shields.io/badge/Network-Base-blue)](https://base.org)
[![Aerodrome](https://img.shields.io/badge/DEX-Aerodrome-purple)](https://aerodrome.finance)
[![SQL Sink](https://img.shields.io/badge/Sink-PostgreSQL%20%7C%20ClickHouse-green)](https://docs.substreams.dev)

Substreams for Aerodrome Finance on Base, covering **both** Aerodrome venues and the
**Coinbase tokenized stocks** that launched on it.

| Venue | Events | Covered |
|---|---|---|
| **Aerodrome v2** — volatile / stable AMM | `Swap`, `Mint`, `Burn`, `Sync` | ✅ |
| **Slipstream** — concentrated liquidity | `Swap`, `Mint`, `Burn`, `Collect` | ✅ |
| **Base B20** — tokenized stocks and other RWAs | 19 token events incl. corporate actions | ✅ |

## Tokenized stocks

Coinbase's tokenized stocks are [B20](https://github.com/base/base-std/tree/main/docs/B20)
tokens, and on Aerodrome they trade **exclusively in Slipstream concentrated-liquidity
pools**. Slipstream inherits Uniswap V3's event ABI, which is a different `Swap` signature
from the v2 AMM:

| Venue | `Swap` signature | topic0 |
|---|---|---|
| Aerodrome v2 | `Swap(address,address,uint256,uint256,uint256,uint256)` | `0xb3e27736…` |
| Slipstream | `Swap(address,address,int256,int256,uint160,uint128,int24)` | `0xc42079f9…` |

A v2-only decoder shares no topic with the stock pools, so it sees none of that volume.
Both are decoded here.

### New tickers index themselves

Pools are discovered from factory `PoolCreated` events rather than from a hardcoded
list, and B20-ness is recovered from the token address itself. Every B20 is created by
the singleton `IB20Factory` precompile at a deterministic address:

```
[ B20 prefix: 10 bytes ][ variant: 1 byte ][ bytes9(keccak256(deployer, salt)) ]
  0xb2 + nine 0x00        0x00 = Asset
                          0x01 = Stablecoin
```

So when a new tokenized asset lists and someone opens an Aerodrome pool for it, the pool
appears in the registry on the block it is created, already flagged — **no code change, no
repackage, no resync.** The variant byte is read straight from the address, so `Asset` and
`Stablecoin` tokens are distinguished with no RPC call, and a variant introduced by a later
hardfork still indexes rather than being dropped.

> Matching on 11 bytes — the prefix plus a hardcoded `Asset` variant byte — is a tempting
> shortcut and a wrong one: it silently rejects every `Stablecoin` and every future variant.
> Verified against the live precompile, which reports `isB20 == true` for the 10-byte prefix
> with any variant byte and `false` as soon as the prefix itself differs.

### Corporate actions

B20 handles splits and reinvested dividends with a **multiplier** rather than by rewriting
balances. That distinction drives how this package models price:

- `balanceOf`, `transfer` and `totalSupply` stay **raw**, so pool reserves and the raw pool
  price are mechanically **unaffected** by a multiplier change. Raw OHLCV stays continuous
  across a split by construction.
- The **share-denominated** price is the raw price divided by the multiplier in force. A 2:1
  split halves it.

`b20_multiplier_schedule` records the timeline and `b20_effective_multiplier(token, ts)`
resolves it. Two details matter:

- A scheduled update names a **future** `effectiveAt`, and the flip is **lazy** — nothing is
  emitted when it actually takes effect. Resolve by timestamp, never by block.
- The spec asks indexers to treat effects emitted **outside** an `Announcement` /
  `EndAnnouncement` bracket as emergency overrides. Every event carries `in_announcement`,
  and `b20_unannounced_actions` surfaces them.

Both multiplier generations are decoded: Beryl (live today) emits only the deprecated
`MultiplierUpdated`, while Cobalt adds ERC-8056's `UIMultiplierUpdated`.

### Metadata is mutable

Every one of the 13 tokenized stocks issued so far has been **renamed at least once** —
`AAPLc` was created as `AAPL` and renamed at block 49,485,877. A symbol captured once at
pool creation goes stale, so `NameUpdated` / `SymbolUpdated` are indexed and the
`b20_token_current` view resolves the live value.

### What B20-ness does and does not prove

`createB20` is **permissionless** — anyone can create a B20 and name it anything. The
address prefix proves a token is in the B20 namespace; it does **not** prove who issued it.
Confirm the issuer before presenting a pool as a given company's stock. `stock_pools`
narrows to the `asset` variant, which is a necessary filter, not a sufficient one.

## Architecture

```
sf.ethereum.type.v2.Block
│
├─► map_pools ─────► Pools               PoolCreated from 3 factories
│      └─► store_pools                   registry: tokens, venue, B20 flag
│
├─► map_swaps ─────► SwapEvents          v2, enriched from the registry
├─► map_liquidity ─► LiquidityEvents     v2 Mint / Burn
├─► map_syncs ─────► SyncEvents          v2 reserves
│
├─► map_cl_swaps ──► ClSwaps             Slipstream — tokenized stocks
├─► map_cl_liquidity ► ClLiquidityEvents Slipstream Mint / Burn / Collect
│
├─► map_b20_events ► B20Events           token events + multiplier timeline
│
└─► db_out ────────► DatabaseChanges     PostgreSQL / ClickHouse
```

### Why every handler gates on the registry

Aerodrome inherits event signatures from the protocols it descends from, so **a topic match
alone does not mean the log came from an Aerodrome pool**:

| Event | Collides with | Measured |
|---|---|---|
| Slipstream `Swap` | Uniswap V3 and every V3 fork | byte-identical signature |
| v2 `Mint(address,uint256,uint256)` | Uniswap V2 and every V2 fork | only **8 of 20** sampled Base emitters were Aerodrome pools |
| v2 `Burn` | Uniswap V2 forks | contaminated in sampling |
| v2 `Swap` | — | reordered params; effectively unique on Base |

Membership in the factory-built registry is what actually identifies a pool, so every handler
skips logs from pools it does not know. An ungated `Mint` handler would attribute the majority
of its rows to contracts that are not Aerodrome at all.

The registry starts at the v2 `PoolFactory` deployment, so it is complete. **Raising
`initialBlock` above that trades completeness for backfill speed** — pools created before the
new start are unknown, and their events are skipped.

## Contract addresses (Base)

Every address below was verified by calling it on Base.

| Contract | Address | Deployed |
|---|---|---|
| v2 `PoolFactory` | `0x420DD381b31aEf6683db6B902084cB0FFECe40Da` | block 3,200,559 |
| Slipstream `CLFactory` v1 | `0x5e7BB104d84c7CB9B682AaC2F3d509f5F406809A` | block 13,843,704 |
| Slipstream `CLFactory` v2 | `0xf8f2eB4940CFE7d13603DDDD87f123820Fc061Ef` | block 44,394,724 |
| `Voter` | `0x16613524e02ad97eDfeF371bC883F2F5d6C480A5` | |
| `AERO` | `0x940181a94A35A4569E4529A3CDfB74e38FD98631` | block 3,200,550 |
| `IB20Factory` precompile | `0xB20f000000000000000000000000000000000000` | |

Both CL factories are live and share the same `Voter`. Every tokenized-stock pool at launch
was created by **CLFactory v2**, so a listener watching only v1 sees none of them.

## Quick start

```bash
substreams auth
substreams build

# Slipstream swaps, tokenized stocks included
substreams run substreams.yaml map_cl_swaps -e base -s 49273390 -t +5000

# Pool discovery — watch stock pools appear
substreams run substreams.yaml map_pools -e base -s 49273390 -t +250

# B20 token events: renames, corporate actions, halts
substreams run substreams.yaml map_b20_events -e base -s 49480000 -t +10000
```

### Backfill cost

`initialBlock` is the v2 `PoolFactory` deployment (**3,200,559**) so the registry covers all of
Aerodrome's history. To index tokenized stocks only, set every module's `initialBlock` to
**44,394,724** (CLFactory v2). Every stock pool lives on that factory, and the backfill is
roughly 10× shorter.

## Stream to SQL

```bash
substreams-sink-sql setup "psql://postgres:password@localhost:5432/aerodrome?sslmode=disable" \
  aerodrome-substreams-v0.3.0.spkg
substreams-sink-sql run   "psql://postgres:password@localhost:5432/aerodrome?sslmode=disable" \
  aerodrome-substreams-v0.3.0.spkg

# ClickHouse
substreams-sink-sql setup "clickhouse://default:@localhost:9000/default" \
  aerodrome-substreams-v0.3.0.spkg --engine=clickhouse
```

### Tables

| Table | Contents |
|---|---|
| `pools` | Pool registry with token metadata and the B20 flag |
| `aerodrome_swaps` | v2 swaps |
| `aerodrome_cl_swaps` | Slipstream swaps, with `sqrtPriceX96` and tick |
| `aerodrome_cl_liquidity` | Slipstream `Mint` / `Burn` / `Collect` |
| `b20_events` | Every B20 token event, flattened |
| `b20_tokens` | `B20Created` birth certificates |
| `b20_multiplier_schedule` | Corporate-action timeline |

### Views

| View | Answers |
|---|---|
| `stock_pools` | Which pools trade tokenized stocks |
| `stock_swaps` | Every stock trade, priced in the quote asset |
| `stock_candles(interval)` | OHLCV at any bucket size (300 / 3600 / 14400 / 86400) |
| `stock_pools_24h` | 24h volume, trades, high / low per stock pool |
| `b20_token_current` | Live name and symbol, honouring renames |
| `b20_corporate_actions` | Splits and dividends, with the announcement that disclosed them |
| `b20_unannounced_actions` | Changes made outside an announcement bracket |
| `b20_pause_history` | Trading halts |
| `cl_swaps_priced` | All Slipstream swaps with a decimal-adjusted price |

```sql
-- What are the tokenized stocks doing today?
SELECT b20_symbol, trades_24h, volume_24h, high_24h, low_24h FROM stock_pools_24h;

-- Hourly candles for one stock pool
SELECT * FROM stock_candles(3600, '0xa3b1e3f9747065e2073722ff4c9027d3ea4994f0');

-- Any pending corporate action?
SELECT symbol, new_multiplier, effective_at FROM b20_corporate_actions WHERE pending;
```

Prices are derived from `sqrtPriceX96` — the pool's post-trade price — rather than from an
output/input ratio, which yields a price in one direction and its reciprocal in the other and
makes a naive candle series alternate between the two.

`token0_decimals` / `token1_decimals` are `-1` when the metadata call failed, so a missing
value is never mistaken for 0 decimals. Price views return `NULL` rather than a wrong number
in that case.

## Development

```bash
cargo test          # unit tests
substreams build    # protobuf + ABI codegen + wasm
substreams pack     # .spkg
```

Event decoders are generated from `abi/*.json` by `build.rs` using `substreams-ethereum`'s
Abigen, so a topic0 cannot drift from the deployed contract the way a hand-written matcher can.

```
abi/
  pool.json          Aerodrome v2 Pool
  pool_factory.json  Aerodrome v2 PoolFactory
  cl_pool.json       Slipstream CLPool
  cl_factory.json    Slipstream CLFactory
  b20.json           Base B20 token
  b20_factory.json   IB20Factory precompile
  erc20.json         decimals() / symbol()
```

ABIs were taken from the official sources — [`aerodrome-finance/contracts`](https://github.com/aerodrome-finance/contracts),
[`aerodrome-finance/slipstream`](https://github.com/aerodrome-finance/slipstream) and
[`base/base-std`](https://github.com/base/base-std) — and every topic0 was confirmed against
live Base logs.
