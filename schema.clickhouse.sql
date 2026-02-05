-- Aerodrome Finance Substreams ClickHouse Schema
-- Optimized for high-performance analytics with materialized views

-- ====================
-- Core Tables
-- ====================

-- Individual swap events
CREATE TABLE IF NOT EXISTS aerodrome_swaps (
    id String,
    tx_hash String,
    log_index UInt64,
    block_number UInt64,
    timestamp Int64,
    pool_address String,
    sender String,
    recipient String,
    amount0_in String,
    amount1_in String,
    amount0_out String,
    amount1_out String
)
ENGINE = MergeTree()
ORDER BY (pool_address, timestamp, tx_hash)
PARTITION BY toYYYYMM(toDateTime(timestamp));

-- ====================
-- OHLCV Candles (Raw)
-- ====================

-- Raw candle data from substreams
CREATE TABLE IF NOT EXISTS candles (
    pool_address String,
    interval_seconds Int64,
    timestamp Int64,
    open Int64,
    high Int64,
    low Int64,
    close Int64,
    volume_in String,
    volume_out String,
    trade_count Int64
)
ENGINE = ReplacingMergeTree()
ORDER BY (pool_address, interval_seconds, timestamp);

-- ====================
-- Aggregation Tables
-- ====================

-- Pool statistics
CREATE TABLE IF NOT EXISTS pool_stats (
    pool_address String,
    swap_count Int64,
    total_volume String,
    last_swap_block UInt64,
    last_swap_time Int64
)
ENGINE = ReplacingMergeTree(last_swap_time)
ORDER BY pool_address;

-- Trader statistics
CREATE TABLE IF NOT EXISTS trader_stats (
    wallet_address String,
    total_swaps Int64,
    total_volume String,
    last_swap_time Int64
)
ENGINE = ReplacingMergeTree(last_swap_time)
ORDER BY wallet_address;

-- Daily statistics
CREATE TABLE IF NOT EXISTS daily_stats (
    date String,
    swap_count Int64,
    total_volume String
)
ENGINE = ReplacingMergeTree()
ORDER BY date;

-- Hourly statistics
CREATE TABLE IF NOT EXISTS hourly_stats (
    hour String,
    swap_count Int64,
    total_volume String
)
ENGINE = ReplacingMergeTree()
ORDER BY hour;

-- Protocol metrics
CREATE TABLE IF NOT EXISTS protocol_metrics (
    protocol String,
    total_swaps Int64,
    total_volume String
)
ENGINE = ReplacingMergeTree()
ORDER BY protocol;

-- ====================
-- Materialized Views for Candle Aggregation
-- ====================

-- Hourly candle aggregation from swaps
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_candles_1h
ENGINE = AggregatingMergeTree()
ORDER BY (pool_address, timestamp)
AS SELECT
    pool_address,
    3600 AS interval_seconds,
    toStartOfHour(toDateTime(timestamp)) AS timestamp,
    argMinState(
        toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1)),
        timestamp
    ) AS open_state,
    argMaxState(
        toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1)),
        timestamp
    ) AS close_state,
    maxState(toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1))) AS high_state,
    minState(toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1))) AS low_state,
    sumState(toUInt64OrZero(amount0_in) + toUInt64OrZero(amount1_in)) AS volume_in_state,
    sumState(toUInt64OrZero(amount0_out) + toUInt64OrZero(amount1_out)) AS volume_out_state,
    countState() AS trade_count_state
FROM aerodrome_swaps
GROUP BY pool_address, timestamp;

-- Daily candle aggregation from swaps
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_candles_1d
ENGINE = AggregatingMergeTree()
ORDER BY (pool_address, timestamp)
AS SELECT
    pool_address,
    86400 AS interval_seconds,
    toStartOfDay(toDateTime(timestamp)) AS timestamp,
    argMinState(
        toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1)),
        timestamp
    ) AS open_state,
    argMaxState(
        toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1)),
        timestamp
    ) AS close_state,
    maxState(toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1))) AS high_state,
    minState(toInt64(toUInt64OrZero(amount0_out) * 1000000 / greatest(toUInt64OrZero(amount0_in), 1))) AS low_state,
    sumState(toUInt64OrZero(amount0_in) + toUInt64OrZero(amount1_in)) AS volume_in_state,
    sumState(toUInt64OrZero(amount0_out) + toUInt64OrZero(amount1_out)) AS volume_out_state,
    countState() AS trade_count_state
FROM aerodrome_swaps
GROUP BY pool_address, timestamp;

-- Pool volume materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_pool_volume
ENGINE = SummingMergeTree()
ORDER BY pool_address
AS SELECT
    pool_address,
    count() AS swap_count,
    sum(toUInt64OrZero(amount0_in) + toUInt64OrZero(amount1_in)) AS total_volume,
    max(block_number) AS last_block,
    max(timestamp) AS last_swap_time
FROM aerodrome_swaps
GROUP BY pool_address;

-- Trader volume materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_trader_volume
ENGINE = SummingMergeTree()
ORDER BY wallet_address
AS SELECT
    sender AS wallet_address,
    count() AS total_swaps,
    sum(toUInt64OrZero(amount0_in) + toUInt64OrZero(amount1_in)) AS total_volume,
    max(timestamp) AS last_swap_time
FROM aerodrome_swaps
GROUP BY sender;

-- Daily volume materialized view
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_daily_volume
ENGINE = SummingMergeTree()
ORDER BY date
AS SELECT
    toDate(toDateTime(timestamp)) AS date,
    count() AS swap_count,
    sum(toUInt64OrZero(amount0_in) + toUInt64OrZero(amount1_in)) AS total_volume
FROM aerodrome_swaps
GROUP BY date;

-- ====================
-- Query Views
-- ====================

-- Finalized hourly candles
CREATE VIEW IF NOT EXISTS v_candles_1h AS
SELECT
    pool_address,
    interval_seconds,
    timestamp,
    argMinMerge(open_state) AS open,
    argMaxMerge(close_state) AS close,
    maxMerge(high_state) AS high,
    minMerge(low_state) AS low,
    sumMerge(volume_in_state) AS volume_in,
    sumMerge(volume_out_state) AS volume_out,
    countMerge(trade_count_state) AS trade_count
FROM mv_candles_1h
GROUP BY pool_address, interval_seconds, timestamp;

-- Finalized daily candles
CREATE VIEW IF NOT EXISTS v_candles_1d AS
SELECT
    pool_address,
    interval_seconds,
    timestamp,
    argMinMerge(open_state) AS open,
    argMaxMerge(close_state) AS close,
    maxMerge(high_state) AS high,
    minMerge(low_state) AS low,
    sumMerge(volume_in_state) AS volume_in,
    sumMerge(volume_out_state) AS volume_out,
    countMerge(trade_count_state) AS trade_count
FROM mv_candles_1d
GROUP BY pool_address, interval_seconds, timestamp;

-- Top pools by volume
CREATE VIEW IF NOT EXISTS v_top_pools AS
SELECT
    pool_address,
    sum(swap_count) AS total_swaps,
    sum(total_volume) AS total_volume
FROM mv_pool_volume
GROUP BY pool_address
ORDER BY total_volume DESC
LIMIT 100;

-- Top traders by volume
CREATE VIEW IF NOT EXISTS v_top_traders AS
SELECT
    wallet_address,
    sum(total_swaps) AS total_swaps,
    sum(total_volume) AS total_volume
FROM mv_trader_volume
GROUP BY wallet_address
ORDER BY total_volume DESC
LIMIT 100;

-- Daily unique traders
CREATE VIEW IF NOT EXISTS v_daily_unique_traders AS
SELECT
    toDate(toDateTime(timestamp)) AS date,
    uniqExact(sender) AS unique_traders,
    count() AS total_swaps
FROM aerodrome_swaps
GROUP BY date
ORDER BY date DESC;

-- Pool activity summary
CREATE VIEW IF NOT EXISTS v_pool_activity AS
SELECT
    pool_address,
    count() AS swap_count,
    uniqExact(sender) AS unique_traders,
    sum(toUInt64OrZero(amount0_in) + toUInt64OrZero(amount1_in)) AS total_volume,
    min(timestamp) AS first_swap,
    max(timestamp) AS last_swap
FROM aerodrome_swaps
GROUP BY pool_address
ORDER BY swap_count DESC;

-- Recent large swaps (whale activity)
CREATE VIEW IF NOT EXISTS v_whale_swaps AS
SELECT
    tx_hash,
    pool_address,
    sender,
    recipient,
    amount0_in,
    amount1_in,
    amount0_out,
    amount1_out,
    toDateTime(timestamp) AS swap_time
FROM aerodrome_swaps
WHERE toUInt64OrZero(amount0_in) + toUInt64OrZero(amount1_in) > 1000000000000
ORDER BY timestamp DESC
LIMIT 100;
