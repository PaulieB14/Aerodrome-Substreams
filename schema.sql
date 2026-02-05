-- Aerodrome Finance Substreams SQL Schema
-- PostgreSQL schema with OHLCV candles and delta updates

-- ====================
-- Core Tables
-- ====================

-- Individual swap events
CREATE TABLE IF NOT EXISTS aerodrome_swaps (
    id VARCHAR PRIMARY KEY,
    tx_hash VARCHAR NOT NULL,
    log_index BIGINT NOT NULL,
    block_number BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    pool_address VARCHAR NOT NULL,
    sender VARCHAR NOT NULL,
    recipient VARCHAR NOT NULL,
    amount0_in VARCHAR NOT NULL,
    amount1_in VARCHAR NOT NULL,
    amount0_out VARCHAR NOT NULL,
    amount1_out VARCHAR NOT NULL,
    amount_in_total BIGINT NOT NULL,
    amount_out_total BIGINT NOT NULL,
    price_ratio BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_swaps_pool ON aerodrome_swaps(pool_address);
CREATE INDEX IF NOT EXISTS idx_swaps_timestamp ON aerodrome_swaps(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_swaps_sender ON aerodrome_swaps(sender);
CREATE INDEX IF NOT EXISTS idx_swaps_block ON aerodrome_swaps(block_number DESC);

-- ====================
-- OHLCV Candles
-- ====================

-- Candles with multiple intervals (5m, 1h, 4h, 1d)
CREATE TABLE IF NOT EXISTS candles (
    pool_address VARCHAR NOT NULL,
    interval_seconds BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    open BIGINT,
    high BIGINT,
    low BIGINT,
    close BIGINT,
    volume_in NUMERIC DEFAULT 0,
    volume_out NUMERIC DEFAULT 0,
    trade_count BIGINT DEFAULT 0,
    PRIMARY KEY (pool_address, interval_seconds, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_candles_pool_interval ON candles(pool_address, interval_seconds);
CREATE INDEX IF NOT EXISTS idx_candles_timestamp ON candles(timestamp DESC);

-- ====================
-- Aggregation Tables
-- ====================

-- Pool statistics
CREATE TABLE IF NOT EXISTS pool_stats (
    pool_address VARCHAR PRIMARY KEY,
    swap_count BIGINT DEFAULT 0,
    total_volume NUMERIC DEFAULT 0,
    last_swap_block BIGINT,
    last_swap_time BIGINT
);

CREATE INDEX IF NOT EXISTS idx_pool_stats_volume ON pool_stats(total_volume DESC);

-- Trader statistics
CREATE TABLE IF NOT EXISTS trader_stats (
    wallet_address VARCHAR PRIMARY KEY,
    total_swaps BIGINT DEFAULT 0,
    total_volume NUMERIC DEFAULT 0,
    last_swap_time BIGINT
);

CREATE INDEX IF NOT EXISTS idx_trader_stats_volume ON trader_stats(total_volume DESC);
CREATE INDEX IF NOT EXISTS idx_trader_stats_swaps ON trader_stats(total_swaps DESC);

-- Daily statistics
CREATE TABLE IF NOT EXISTS daily_stats (
    date VARCHAR PRIMARY KEY,
    swap_count BIGINT DEFAULT 0,
    total_volume NUMERIC DEFAULT 0
);

-- Hourly statistics
CREATE TABLE IF NOT EXISTS hourly_stats (
    hour VARCHAR PRIMARY KEY,
    swap_count BIGINT DEFAULT 0,
    total_volume NUMERIC DEFAULT 0
);

-- Protocol-wide metrics
CREATE TABLE IF NOT EXISTS protocol_metrics (
    protocol VARCHAR PRIMARY KEY,
    total_swaps BIGINT DEFAULT 0,
    total_volume NUMERIC DEFAULT 0
);

-- ====================
-- Views for Common Queries
-- ====================

-- Latest candles (most recent for each pool/interval)
CREATE OR REPLACE VIEW latest_candles AS
SELECT DISTINCT ON (pool_address, interval_seconds)
    pool_address,
    interval_seconds,
    timestamp,
    open,
    high,
    low,
    close,
    volume_in,
    volume_out,
    trade_count
FROM candles
ORDER BY pool_address, interval_seconds, timestamp DESC;

-- Top pools by volume (24h)
CREATE OR REPLACE VIEW top_pools_24h AS
SELECT
    pool_address,
    SUM(volume_in) AS volume_24h,
    SUM(trade_count) AS trades_24h
FROM candles
WHERE interval_seconds = 3600
  AND timestamp > EXTRACT(EPOCH FROM NOW())::BIGINT - 86400
GROUP BY pool_address
ORDER BY volume_24h DESC
LIMIT 50;

-- Top traders by volume
CREATE OR REPLACE VIEW top_traders AS
SELECT
    wallet_address,
    total_swaps,
    total_volume,
    TO_TIMESTAMP(last_swap_time) AS last_active
FROM trader_stats
ORDER BY total_volume DESC
LIMIT 100;

-- Daily volume trend (30 days)
CREATE OR REPLACE VIEW daily_volume_trend AS
SELECT
    date,
    swap_count,
    total_volume
FROM daily_stats
ORDER BY date DESC
LIMIT 30;

-- Hourly volume (24 hours)
CREATE OR REPLACE VIEW hourly_volume_24h AS
SELECT
    hour,
    swap_count,
    total_volume
FROM hourly_stats
ORDER BY hour DESC
LIMIT 24;

-- 5-minute candles (last 24 hours)
CREATE OR REPLACE VIEW candles_5m_24h AS
SELECT *
FROM candles
WHERE interval_seconds = 300
  AND timestamp > EXTRACT(EPOCH FROM NOW())::BIGINT - 86400
ORDER BY pool_address, timestamp DESC;

-- Hourly candles (last 7 days)
CREATE OR REPLACE VIEW candles_1h_7d AS
SELECT *
FROM candles
WHERE interval_seconds = 3600
  AND timestamp > EXTRACT(EPOCH FROM NOW())::BIGINT - 604800
ORDER BY pool_address, timestamp DESC;

-- Daily candles (last 30 days)
CREATE OR REPLACE VIEW candles_1d_30d AS
SELECT *
FROM candles
WHERE interval_seconds = 86400
  AND timestamp > EXTRACT(EPOCH FROM NOW())::BIGINT - 2592000
ORDER BY pool_address, timestamp DESC;

-- ====================
-- Utility Functions
-- ====================

-- Get candles for a specific pool
CREATE OR REPLACE FUNCTION get_candles(
    p_pool VARCHAR,
    p_interval BIGINT,
    p_limit INTEGER DEFAULT 100
)
RETURNS TABLE (
    timestamp BIGINT,
    open BIGINT,
    high BIGINT,
    low BIGINT,
    close BIGINT,
    volume_in NUMERIC,
    volume_out NUMERIC,
    trade_count BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        c.timestamp,
        c.open,
        c.high,
        c.low,
        c.close,
        c.volume_in,
        c.volume_out,
        c.trade_count
    FROM candles c
    WHERE c.pool_address = p_pool
      AND c.interval_seconds = p_interval
    ORDER BY c.timestamp DESC
    LIMIT p_limit;
END;
$$ LANGUAGE plpgsql;

-- Get pool summary
CREATE OR REPLACE FUNCTION get_pool_summary(p_pool VARCHAR)
RETURNS TABLE (
    pool_address VARCHAR,
    swap_count BIGINT,
    total_volume NUMERIC,
    volume_24h NUMERIC,
    trades_24h BIGINT,
    last_swap_time TIMESTAMP
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        ps.pool_address,
        ps.swap_count,
        ps.total_volume,
        COALESCE(SUM(c.volume_in), 0) AS volume_24h,
        COALESCE(SUM(c.trade_count), 0) AS trades_24h,
        TO_TIMESTAMP(ps.last_swap_time) AS last_swap_time
    FROM pool_stats ps
    LEFT JOIN candles c ON c.pool_address = ps.pool_address
        AND c.interval_seconds = 3600
        AND c.timestamp > EXTRACT(EPOCH FROM NOW())::BIGINT - 86400
    WHERE ps.pool_address = p_pool
    GROUP BY ps.pool_address, ps.swap_count, ps.total_volume, ps.last_swap_time;
END;
$$ LANGUAGE plpgsql;
