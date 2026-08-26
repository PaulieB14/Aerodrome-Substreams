-- Aerodrome Finance Substreams -- ClickHouse schema
--
-- Raw tables are written by the `db_out` module; aggregates are derived views.
-- Run the sink with `--engine=clickhouse`.

-- ===========================================================================
-- Pool registry
-- ===========================================================================

CREATE TABLE IF NOT EXISTS pools (
    id                String,
    address           String,
    token0            String,
    token1            String,
    pool_type         String,
    tick_spacing      Int32,
    factory           String,
    is_b20            Bool,
    b20_token         String,
    b20_variant       String,
    created_block     UInt64,
    created_timestamp Int64,
    created_tx        String,
    -- -1 means the metadata call failed; never read it as 0 decimals.
    token0_decimals   Int32,
    token1_decimals   Int32,
    token0_symbol     String,
    token1_symbol     String
) ENGINE = ReplacingMergeTree()
ORDER BY id;

-- ===========================================================================
-- Aerodrome v2
-- ===========================================================================

CREATE TABLE IF NOT EXISTS aerodrome_swaps (
    id                      String,
    tx_hash                 String,
    log_index               UInt64,
    block_number            UInt64,
    timestamp               Int64,
    pool_address            String,
    sender                  String,
    recipient               String,
    -- uint256 amounts. A 64-bit column would silently drop any trade above
    -- ~18.4 tokens of an 18-decimal asset.
    amount0_in              UInt256,
    amount1_in              UInt256,
    amount0_out             UInt256,
    amount1_out             UInt256,
    amount_in_total         UInt256,
    amount_out_total        UInt256,
    price_token1_per_token0 String,
    token0                  String,
    token1                  String,
    pool_type               String,
    is_b20                  Bool,
    b20_token               String
) ENGINE = ReplacingMergeTree()
ORDER BY (pool_address, timestamp, id);

-- ===========================================================================
-- Slipstream concentrated liquidity -- where tokenized stocks trade
-- ===========================================================================

CREATE TABLE IF NOT EXISTS aerodrome_cl_swaps (
    id             String,
    tx_hash        String,
    log_index      UInt64,
    block_number   UInt64,
    timestamp      Int64,
    pool_address   String,
    sender         String,
    recipient      String,
    -- Signed pool deltas: negative means the pool paid that token out.
    amount0        Int256,
    amount1        Int256,
    sqrt_price_x96 UInt256,
    liquidity      UInt256,
    tick           Int32,
    token0         String,
    token1         String,
    tick_spacing   Int32,
    is_b20         Bool,
    b20_token      String
) ENGINE = ReplacingMergeTree()
ORDER BY (pool_address, timestamp, id);

CREATE TABLE IF NOT EXISTS aerodrome_cl_liquidity (
    id           String,
    tx_hash      String,
    log_index    UInt64,
    block_number UInt64,
    timestamp    Int64,
    pool_address String,
    action       String,
    owner        String,
    sender       String,
    recipient    String,
    tick_lower   Int32,
    tick_upper   Int32,
    amount       String,
    amount0      Int256,
    amount1      Int256,
    token0       String,
    token1       String,
    is_b20       Bool,
    b20_token    String
) ENGINE = ReplacingMergeTree()
ORDER BY (pool_address, timestamp, id);

-- ===========================================================================
-- Base B20 tokens
-- ===========================================================================

CREATE TABLE IF NOT EXISTS b20_tokens (
    id                String,
    address           String,
    variant           String,
    name              String,
    symbol            String,
    decimals          String,
    created_block     UInt64,
    created_timestamp Int64,
    deployer          String
) ENGINE = ReplacingMergeTree()
ORDER BY id;

CREATE TABLE IF NOT EXISTS b20_events (
    id              String,
    tx_hash         String,
    log_index       UInt64,
    block_number    UInt64,
    timestamp       Int64,
    token           String,
    variant         String,
    event_type      String,
    actor           String,
    account         String,
    recipient       String,
    amount          String,
    old_value       String,
    new_value       String,
    effective_at    Int64,
    key             String,
    text            String,
    announcement_id String,
    uri             String,
    -- False means the operation ran outside an announcement bracket, which the
    -- B20 spec asks indexers to surface as an emergency override.
    in_announcement Bool,
    pause_features  String
) ENGINE = ReplacingMergeTree()
ORDER BY (token, block_number, id);

-- `effective_at` may be in the future: the multiplier flip is lazy and emits
-- nothing when it happens, so resolve by timestamp rather than by block.
CREATE TABLE IF NOT EXISTS b20_multiplier_schedule (
    id                  String,
    token               String,
    old_multiplier      String,
    new_multiplier      String,
    effective_at        Int64,
    announced_block     UInt64,
    announced_timestamp Int64,
    tx_hash             String,
    log_index           UInt64,
    scheduled           Bool,
    cancelled           Bool,
    announcement_id     String
) ENGINE = ReplacingMergeTree()
ORDER BY (token, effective_at, id);

-- ===========================================================================
-- Views
-- ===========================================================================

-- Current name and symbol per B20, honouring post-launch renames.
-- Every tokenized stock issued so far has been renamed at least once
-- (AAPLc was created as `AAPL`), so this is the normal path, not an edge case.
CREATE VIEW IF NOT EXISTS b20_token_current AS
SELECT
    token,
    anyLast(variant) AS variant,
    argMaxOrNull(text, if(event_type = 'name_updated',   toInt64(block_number), toInt64(-1))) AS name,
    argMaxOrNull(text, if(event_type = 'symbol_updated', toInt64(block_number), toInt64(-1))) AS symbol
FROM b20_events
GROUP BY token;

-- Aerodrome pools holding a B20, with the B20 side identified.
CREATE VIEW IF NOT EXISTS b20_pools AS
SELECT
    p.address       AS address,
    p.pool_type     AS pool_type,
    p.tick_spacing  AS tick_spacing,
    p.b20_token     AS b20_token,
    p.b20_variant   AS b20_variant,
    p.token0        AS token0,
    p.token1        AS token1,
    if(p.b20_token = p.token0, p.token1, p.token0)                   AS quote_token,
    if(p.b20_token = p.token0, p.token1_symbol, p.token0_symbol)     AS quote_symbol,
    if(p.b20_token = p.token0, p.token1_decimals, p.token0_decimals) AS quote_decimals,
    if(p.b20_token = p.token0, p.token0_decimals, p.token1_decimals) AS b20_decimals,
    coalesce(
        nullIf(c.symbol, ''),
        if(p.b20_token = p.token0, p.token0_symbol, p.token1_symbol)
    ) AS b20_symbol
FROM pools AS p
LEFT JOIN b20_token_current AS c ON c.token = p.b20_token
WHERE p.is_b20;

-- Tokenized-stock pools: the B20 `asset` variant.
-- `createB20` is permissionless, so B20-ness alone does not identify the issuer.
CREATE VIEW IF NOT EXISTS stock_pools AS
SELECT * FROM b20_pools WHERE b20_variant = 'asset';

-- Slipstream swaps with a price attached.
-- Price comes from sqrtPriceX96 -- the pool's post-trade price -- so it is
-- direction-independent, unlike an output/input ratio.
CREATE VIEW IF NOT EXISTS cl_swaps_priced AS
SELECT
    s.id           AS id,
    s.tx_hash      AS tx_hash,
    s.block_number AS block_number,
    s.timestamp    AS timestamp,
    s.pool_address AS pool_address,
    s.sender       AS sender,
    s.recipient    AS recipient,
    s.amount0      AS amount0,
    s.amount1      AS amount1,
    s.token0       AS token0,
    s.token1       AS token1,
    if(p.token0_decimals >= 0 AND p.token1_decimals >= 0,
       pow(toFloat64(s.sqrt_price_x96) / pow(2, 96), 2)
         * pow(10, p.token0_decimals - p.token1_decimals),
       NULL) AS price_token1_per_token0
FROM aerodrome_cl_swaps AS s
INNER JOIN pools AS p ON p.address = s.pool_address;

-- Tokenized-stock trades, priced in the quote asset.
--
-- `price_raw` is the price of one whole B20 unit and stays continuous across a
-- corporate action, because the multiplier is cosmetic to the AMM -- pool
-- reserves are raw. `price_per_share` divides by the multiplier in force, so a
-- split shows as a change in shares per token instead of a phantom price gap.
CREATE VIEW IF NOT EXISTS stock_swaps AS
SELECT
    s.id           AS id,
    s.tx_hash      AS tx_hash,
    s.block_number AS block_number,
    s.timestamp    AS timestamp,
    s.pool_address AS pool_address,
    s.sender       AS sender,
    bp.b20_token   AS b20_token,
    bp.b20_symbol  AS b20_symbol,
    bp.quote_symbol AS quote_symbol,
    if(bp.b20_token = s.token0, s.amount0, s.amount1) AS b20_amount,
    if(bp.b20_token = s.token0, s.amount1, s.amount0) AS quote_amount,
    if(bp.b20_token = s.token0,
       s.price_token1_per_token0,
       if(s.price_token1_per_token0 > 0, 1 / s.price_token1_per_token0, NULL)) AS price_raw,
    price_raw / (if(m.new_multiplier = '', 1e18, toFloat64(m.new_multiplier)) / 1e18) AS price_per_share
FROM cl_swaps_priced AS s
INNER JOIN stock_pools AS bp ON bp.address = s.pool_address
-- ASOF resolves the multiplier in force at each swap's own timestamp. A plain
-- join against the latest row would misprice every trade that happened before
-- the most recent corporate action.
ASOF LEFT JOIN (
    SELECT token, new_multiplier, effective_at
    FROM b20_multiplier_schedule
    WHERE NOT cancelled
) AS m ON m.token = bp.b20_token AND m.effective_at <= s.timestamp;

-- Hourly OHLCV for tokenized stocks.
CREATE VIEW IF NOT EXISTS stock_candles_1h AS
SELECT
    pool_address,
    b20_symbol,
    intDiv(timestamp, 3600) * 3600 AS bucket,
    argMin(price_raw, block_number) AS open,
    max(price_raw)                  AS high,
    min(price_raw)                  AS low,
    argMax(price_raw, block_number) AS close,
    sum(abs(b20_amount))            AS volume,
    count()                         AS trades
FROM stock_swaps
WHERE isNotNull(price_raw)
GROUP BY pool_address, b20_symbol, bucket;

CREATE VIEW IF NOT EXISTS stock_candles_1d AS
SELECT
    pool_address,
    b20_symbol,
    intDiv(timestamp, 86400) * 86400 AS bucket,
    argMin(price_raw, block_number)  AS open,
    max(price_raw)                   AS high,
    min(price_raw)                   AS low,
    argMax(price_raw, block_number)  AS close,
    sum(abs(b20_amount))             AS volume,
    count()                          AS trades
FROM stock_swaps
WHERE isNotNull(price_raw)
GROUP BY pool_address, b20_symbol, bucket;

-- Corporate actions, newest first.
CREATE VIEW IF NOT EXISTS b20_corporate_actions AS
SELECT
    m.token                                AS token,
    c.symbol                               AS symbol,
    toFloat64(m.old_multiplier) / 1e18     AS old_multiplier,
    toFloat64(m.new_multiplier) / 1e18     AS new_multiplier,
    m.effective_at                         AS effective_at,
    m.announced_timestamp                  AS announced_timestamp,
    m.scheduled                            AS scheduled,
    m.cancelled                            AS cancelled,
    m.announcement_id                      AS announcement_id
FROM b20_multiplier_schedule AS m
LEFT JOIN b20_token_current AS c ON c.token = m.token;

-- Operations that changed a token with no announcement bracket around them.
CREATE VIEW IF NOT EXISTS b20_unannounced_actions AS
SELECT * FROM b20_events
WHERE NOT in_announcement
  AND event_type IN ('multiplier_updated', 'ui_multiplier_updated',
                     'ui_multiplier_update_cancelled', 'paused', 'unpaused',
                     'seized', 'burned_blocked');

-- Trading halts, expected around a reverse split.
CREATE VIEW IF NOT EXISTS b20_pause_history AS
SELECT token, event_type, pause_features, actor, timestamp, block_number, in_announcement
FROM b20_events
WHERE event_type IN ('paused', 'unpaused');

CREATE VIEW IF NOT EXISTS cl_pool_stats AS
SELECT
    pool_address,
    count()             AS swap_count,
    sum(abs(amount0))   AS total_volume_token0,
    sum(abs(amount1))   AS total_volume_token1,
    max(timestamp)      AS last_swap_time
FROM aerodrome_cl_swaps
GROUP BY pool_address;

CREATE VIEW IF NOT EXISTS pool_stats AS
SELECT
    pool_address,
    count()               AS swap_count,
    sum(amount_in_total)  AS total_volume,
    max(timestamp)        AS last_swap_time
FROM aerodrome_swaps
GROUP BY pool_address;

CREATE VIEW IF NOT EXISTS trader_stats AS
SELECT
    sender AS wallet_address,
    count()              AS total_swaps,
    sum(amount_in_total) AS total_volume,
    max(timestamp)       AS last_swap_time
FROM aerodrome_swaps
GROUP BY sender;
