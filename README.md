# Aerodrome Finance Substreams

[![Substreams](https://img.shields.io/badge/Substreams-v0.2.0-blue)](https://substreams.dev)
[![Base](https://img.shields.io/badge/Network-Base-blue)](https://base.org)
[![Aerodrome](https://img.shields.io/badge/DEX-Aerodrome-purple)](https://aerodrome.finance)
[![SQL Sink](https://img.shields.io/badge/Sink-PostgreSQL%20%7C%20ClickHouse-green)](https://docs.substreams.dev)

High-performance Substreams for tracking Aerodrome Finance DEX on Base with **OHLCV candles**, **SQL sink support**, **delta updates**, and comprehensive swap analytics.

## Features

| Feature | Description |
|---------|-------------|
| **OHLCV Candles** | Real-time candlestick data at 5min, 1hr, 4hr, and daily intervals |
| **SQL Database Sink** | Stream directly to PostgreSQL or ClickHouse |
| **Delta Updates** | Efficient aggregations using `set_if_null`, `set`, `max`, `min`, `add` operations |
| **Liquidity Events** | Track Mint/Burn events for LP activity |
| **Sync Events** | Monitor reserve updates across pools |
| **Persistent Stores** | Track volumes, unique traders, and pool stats across blocks |
| **Production Ready** | Optimized Rust with unit tests and comprehensive error handling |

## Quick Start

### Install & Authenticate

```bash
# Install Substreams CLI
curl -sSL https://substreams.dev/install.sh | bash

# Authenticate with StreamingFast
substreams auth
```

### Run Analytics

```bash
# Stream Aerodrome swap events
substreams run aerodrome-substreams-v0.2.0.spkg \
  map_swaps \
  -e base.substreams.pinax.network:443 \
  -s 10000000 -t +100
```

### Stream to PostgreSQL

```bash
# 1. Start PostgreSQL
docker run -d --name postgres \
  -e POSTGRES_DB=aerodrome \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=password \
  -p 5432:5432 \
  postgres:15

# 2. Setup schema
substreams-sink-sql setup \
  "psql://postgres:password@localhost:5432/aerodrome?sslmode=disable" \
  aerodrome-substreams-v0.2.0.spkg

# 3. Run sink
substreams-sink-sql run \
  "psql://postgres:password@localhost:5432/aerodrome?sslmode=disable" \
  aerodrome-substreams-v0.2.0.spkg
```

### Stream to ClickHouse

```bash
# 1. Start ClickHouse
docker run -d --name clickhouse \
  -p 8123:8123 -p 9000:9000 \
  clickhouse/clickhouse-server:latest

# 2. Setup and run with ClickHouse engine
substreams-sink-sql setup \
  "clickhouse://default:@localhost:9000/default" \
  aerodrome-substreams-v0.2.0.spkg \
  --engine=clickhouse

substreams-sink-sql run \
  "clickhouse://default:@localhost:9000/default" \
  aerodrome-substreams-v0.2.0.spkg \
  --engine=clickhouse
```

## Architecture

```
sf.ethereum.type.v2.Block
│
├─► map_swaps ──► SwapEvents
│   │
│   ├─► store_swap_volumes (bigint, add)
│   │   └─► pool:{addr}:volume, daily:{date}:volume, total:volume
│   │
│   ├─► store_unique_traders (string, set_if_not_exists)
│   │   └─► trader:{wallet}, daily:{date}:trader:{wallet}
│   │
│   ├─► store_pool_stats (bigint, add)
│   │   └─► pool:{addr}:trade_count
│   │
│   └─► db_out ──► DatabaseChanges (SQL Sink)
│       │
│       ├─► aerodrome_swaps (individual trades)
│       ├─► candles (OHLCV at 5m/1h/4h/1d)
│       ├─► pool_stats (per-pool metrics)
│       ├─► trader_stats (wallet activity)
│       ├─► daily_stats / hourly_stats
│       └─► protocol_metrics (global totals)
│
├─► map_liquidity ──► LiquidityEvents
│   └─► Mint/Burn events for LP tracking
│
└─► map_syncs ──► SyncEvents
    └─► Reserve updates across pools
```

## Database Schema

### Core Tables

| Table | Description | Delta Operations |
|-------|-------------|------------------|
| `aerodrome_swaps` | Individual swap events | `create_row` |
| `candles` | OHLCV candlestick data | `set_if_null(open)`, `set(close)`, `max(high)`, `min(low)`, `add(volume)` |
| `pool_stats` | Per-pool statistics | `add(swap_count, volume)`, `set(last_swap)` |
| `trader_stats` | Wallet activity | `add(swaps, volume)`, `set(last_swap)` |
| `daily_stats` | Daily aggregations | `add(swap_count, volume)` |
| `hourly_stats` | Hourly aggregations | `add(swap_count, volume)` |
| `protocol_metrics` | Global protocol metrics | `add(swaps, volume)` |

### Candle Intervals

| Interval | Seconds | Use Case |
|----------|---------|----------|
| 5 minutes | 300 | High-frequency trading, scalping |
| 1 hour | 3600 | Intraday analysis |
| 4 hours | 14400 | Swing trading |
| 1 day | 86400 | Long-term trends |

### Views (PostgreSQL)

```sql
-- Latest candles for all pools
SELECT * FROM latest_candles;

-- Top pools by 24h volume
SELECT * FROM top_pools_24h;

-- Top traders by volume
SELECT * FROM top_traders;

-- Daily volume trend (30 days)
SELECT * FROM daily_volume_trend;

-- 5-minute candles (24h)
SELECT * FROM candles_5m_24h WHERE pool_address = '0x...';

-- Hourly candles (7 days)
SELECT * FROM candles_1h_7d WHERE pool_address = '0x...';

-- Daily candles (30 days)
SELECT * FROM candles_1d_30d WHERE pool_address = '0x...';

-- Get candles for a specific pool
SELECT * FROM get_candles('0x...', 3600, 24);
```

### Views (ClickHouse)

```sql
-- Finalized hourly candles
SELECT * FROM v_candles_1h WHERE pool_address = '0x...' ORDER BY timestamp DESC;

-- Finalized daily candles
SELECT * FROM v_candles_1d WHERE pool_address = '0x...' ORDER BY timestamp DESC;

-- Top pools by volume
SELECT * FROM v_top_pools;

-- Top traders
SELECT * FROM v_top_traders;

-- Daily unique traders
SELECT * FROM v_daily_unique_traders;

-- Whale activity (large swaps)
SELECT * FROM v_whale_swaps;
```

## Contract Addresses (Base)

| Contract | Address |
|----------|---------|
| **Router** | `0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43` |
| **Factory** | `0x420DD381b31aEf6683db6B902084cB0FFECe40Da` |
| **Voter** | `0x16613524e02ad97eDfeF371bC883F2F5d6C480A5` |
| **AERO Token** | `0x940181a94A35A4569E4529A3CDfB74e38FD98631` |
| **Gauge Factory** | `0x35f35cA5B132CaDf2916BaB57639128eAC5bbcb5` |

## Aerodrome Events Tracked

| Event | Description |
|-------|-------------|
| **Swap** | Token swaps with amount0_in/out, amount1_in/out |
| **Mint** | Liquidity additions (LP deposits) |
| **Burn** | Liquidity removals (LP withdrawals) |
| **Sync** | Reserve updates after any pool state change |

## Example Queries

### Get Candles for a Pool

```sql
-- PostgreSQL: Use the helper function
SELECT * FROM get_candles('0xPoolAddress', 3600, 24);

-- Or query directly
SELECT
    timestamp,
    open,
    high,
    low,
    close,
    volume_in,
    trade_count
FROM candles
WHERE pool_address = '0xPoolAddress'
  AND interval_seconds = 3600
ORDER BY timestamp DESC
LIMIT 24;
```

### Top Pools by Volume

```sql
-- PostgreSQL
SELECT
    pool_address,
    swap_count,
    total_volume,
    TO_TIMESTAMP(last_swap_time) AS last_active
FROM pool_stats
ORDER BY total_volume DESC
LIMIT 20;

-- ClickHouse
SELECT * FROM v_top_pools;
```

### Whale Activity (Large Trades)

```sql
SELECT
    tx_hash,
    pool_address,
    sender,
    amount0_in,
    amount1_in,
    amount0_out,
    amount1_out,
    TO_TIMESTAMP(timestamp) as swap_time
FROM aerodrome_swaps
WHERE CAST(amount0_in AS NUMERIC) + CAST(amount1_in AS NUMERIC) > 1000000000000
ORDER BY timestamp DESC
LIMIT 100;
```

### Trader Analysis

```sql
SELECT
    wallet_address,
    total_swaps,
    total_volume,
    TO_TIMESTAMP(last_swap_time) as last_active
FROM trader_stats
WHERE total_swaps > 10
ORDER BY total_volume DESC
LIMIT 50;
```

## Development

### Prerequisites

- Rust 1.70+
- Substreams CLI 1.7.0+
- `buf` CLI (for protobuf generation)

### Build

```bash
# Clone repository
git clone https://github.com/PaulieB14/Aerodrome-Substreams.git
cd Aerodrome-Substreams

# Generate protobuf and ABI code
buf generate proto
cargo build

# Build WASM
substreams build

# Run tests
cargo test

# Run with GUI
substreams gui substreams.yaml map_swaps \
  -e base.substreams.pinax.network:443 \
  -s 10000000 -t +100
```

### Project Structure

```
Aerodrome-Substreams/
├── src/
│   ├── lib.rs              # Module exports and map handlers
│   ├── stores.rs           # Persistent store handlers
│   ├── abi/                # Generated ABI code
│   │   └── pool.rs         # Pool contract events
│   └── pb/                 # Generated protobuf
│       └── aerodrome.rs    # Proto types
├── proto/
│   └── aerodrome.proto     # Data type definitions
├── abi/
│   └── pool.json           # Pool ABI (Swap, Mint, Burn, Sync)
├── schema.sql              # PostgreSQL schema
├── schema.clickhouse.sql   # ClickHouse schema
├── substreams.yaml         # Manifest
├── buf.gen.yaml            # Protobuf generation config
├── build.rs                # ABI code generation
└── Cargo.toml              # Dependencies
```

## What's New in v0.2.0

### OHLCV Candles
- Real-time candlestick data for all pools
- Multiple intervals: 5min, 1hr, 4hr, 1day
- Delta updates: `set_if_null(open)`, `set(close)`, `max(high)`, `min(low)`, `add(volume)`

### SQL Sink Support
- PostgreSQL schema with views and utility functions
- ClickHouse schema with materialized views for real-time aggregation

### Enhanced Event Parsing
- Swap events with full amount tracking
- Mint/Burn events for LP monitoring
- Sync events for reserve tracking

### Persistent Stores
- `store_swap_volumes` - Cumulative volumes by pool and date
- `store_unique_traders` - First-seen tracking for wallets
- `store_pool_stats` - Trade counts per pool

### ClickHouse Optimizations
- Materialized views for real-time candle aggregation
- `AggregatingMergeTree` for efficient state management
- Pre-computed views for common queries

## Resources

- [Substreams Documentation](https://docs.substreams.dev/)
- [SQL Sink Guide](https://docs.substreams.dev/documentation/consume/sql)
- [Delta Updates Demo](https://github.com/streamingfast/substreams-eth-uni-v4-demo-candles)
- [Aerodrome Finance](https://aerodrome.finance/)
- [Base Documentation](https://docs.base.org/)

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/awesome`)
3. Run tests (`cargo test`)
4. Commit changes (`git commit -m 'Add awesome feature'`)
5. Push to branch (`git push origin feature/awesome`)
6. Open a Pull Request

## License

MIT License - see [LICENSE](LICENSE) for details.

---

**Built with Substreams for the Aerodrome Finance and Base ecosystem**
