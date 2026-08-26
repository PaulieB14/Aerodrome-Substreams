-- Aerodrome Finance Substreams -- PostgreSQL schema
--
-- Raw tables are written by the `db_out` module. Everything aggregated
-- (candles, per-pool and per-token stats) is derived here as views, so the
-- numbers can never drift from the events they are built from.

-- ===========================================================================
-- Pool registry
-- ===========================================================================

-- Every Aerodrome pool, discovered from its factory's PoolCreated event.
-- A pool for a tokenized stock that lists next week lands here automatically.
CREATE TABLE IF NOT EXISTS pools (
    id                VARCHAR PRIMARY KEY,
    address           VARCHAR NOT NULL,
    token0            VARCHAR NOT NULL,
    token1            VARCHAR NOT NULL,
    pool_type         VARCHAR NOT NULL,          -- 'cl' | 'v2_stable' | 'v2_volatile'
    tick_spacing      INTEGER NOT NULL DEFAULT 0,
    factory           VARCHAR NOT NULL,
    is_b20            BOOLEAN NOT NULL DEFAULT FALSE,
    b20_token         VARCHAR,
    b20_variant       VARCHAR,                   -- 'asset' | 'stablecoin' | 'unknown'
    created_block     BIGINT  NOT NULL,
    created_timestamp BIGINT  NOT NULL,
    created_tx        VARCHAR,
    -- -1 means the metadata call failed; never treat it as 0 decimals.
    token0_decimals   INTEGER NOT NULL DEFAULT -1,
    token1_decimals   INTEGER NOT NULL DEFAULT -1,
    token0_symbol     VARCHAR,
    token1_symbol     VARCHAR
);

CREATE INDEX IF NOT EXISTS idx_pools_b20     ON pools(is_b20) WHERE is_b20;
CREATE INDEX IF NOT EXISTS idx_pools_type    ON pools(pool_type);
CREATE INDEX IF NOT EXISTS idx_pools_token0  ON pools(token0);
CREATE INDEX IF NOT EXISTS idx_pools_token1  ON pools(token1);

-- ===========================================================================
-- Aerodrome v2 (volatile / stable AMM)
-- ===========================================================================

CREATE TABLE IF NOT EXISTS aerodrome_swaps (
    id                       VARCHAR PRIMARY KEY,
    tx_hash                  VARCHAR NOT NULL,
    log_index                BIGINT  NOT NULL,
    block_number             BIGINT  NOT NULL,
    timestamp                BIGINT  NOT NULL,
    pool_address             VARCHAR NOT NULL,
    sender                   VARCHAR NOT NULL,
    recipient                VARCHAR NOT NULL,
    -- uint256 values: NUMERIC, never BIGINT. Any trade above ~18.4 tokens of an
    -- 18-decimal asset overflows a 64-bit integer.
    amount0_in               NUMERIC NOT NULL,
    amount1_in               NUMERIC NOT NULL,
    amount0_out              NUMERIC NOT NULL,
    amount1_out              NUMERIC NOT NULL,
    amount_in_total          NUMERIC NOT NULL,
    amount_out_total         NUMERIC NOT NULL,
    -- Raw token1-per-token0, orientation-independent. Apply the pool's decimals
    -- for a human price.
    price_token1_per_token0  NUMERIC,
    token0                   VARCHAR,
    token1                   VARCHAR,
    pool_type                VARCHAR,
    is_b20                   BOOLEAN NOT NULL DEFAULT FALSE,
    b20_token                VARCHAR
);

CREATE INDEX IF NOT EXISTS idx_swaps_pool      ON aerodrome_swaps(pool_address);
CREATE INDEX IF NOT EXISTS idx_swaps_timestamp ON aerodrome_swaps(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_swaps_sender    ON aerodrome_swaps(sender);
CREATE INDEX IF NOT EXISTS idx_swaps_block     ON aerodrome_swaps(block_number DESC);
CREATE INDEX IF NOT EXISTS idx_swaps_b20       ON aerodrome_swaps(is_b20) WHERE is_b20;

-- ===========================================================================
-- Slipstream concentrated liquidity -- where tokenized stocks trade
-- ===========================================================================

CREATE TABLE IF NOT EXISTS aerodrome_cl_swaps (
    id              VARCHAR PRIMARY KEY,
    tx_hash         VARCHAR NOT NULL,
    log_index       BIGINT  NOT NULL,
    block_number    BIGINT  NOT NULL,
    timestamp       BIGINT  NOT NULL,
    pool_address    VARCHAR NOT NULL,
    sender          VARCHAR NOT NULL,
    recipient       VARCHAR NOT NULL,
    -- Signed pool deltas: negative means the pool paid that token out.
    amount0         NUMERIC NOT NULL,
    amount1         NUMERIC NOT NULL,
    sqrt_price_x96  NUMERIC NOT NULL,
    liquidity       NUMERIC NOT NULL,
    tick            INTEGER NOT NULL,
    token0          VARCHAR,
    token1          VARCHAR,
    tick_spacing    INTEGER,
    is_b20          BOOLEAN NOT NULL DEFAULT FALSE,
    b20_token       VARCHAR
);

CREATE INDEX IF NOT EXISTS idx_cl_swaps_pool      ON aerodrome_cl_swaps(pool_address);
CREATE INDEX IF NOT EXISTS idx_cl_swaps_timestamp ON aerodrome_cl_swaps(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_cl_swaps_sender    ON aerodrome_cl_swaps(sender);
CREATE INDEX IF NOT EXISTS idx_cl_swaps_b20       ON aerodrome_cl_swaps(is_b20) WHERE is_b20;

CREATE TABLE IF NOT EXISTS aerodrome_cl_liquidity (
    id            VARCHAR PRIMARY KEY,
    tx_hash       VARCHAR NOT NULL,
    log_index     BIGINT  NOT NULL,
    block_number  BIGINT  NOT NULL,
    timestamp     BIGINT  NOT NULL,
    pool_address  VARCHAR NOT NULL,
    action        VARCHAR NOT NULL,             -- 'mint' | 'burn' | 'collect'
    -- The pool-level position owner. For positions opened through the
    -- NonfungiblePositionManager this is the manager, and the end user owns the
    -- corresponding position NFT.
    owner         VARCHAR,
    sender        VARCHAR,
    recipient     VARCHAR,
    tick_lower    INTEGER,
    tick_upper    INTEGER,
    amount        NUMERIC,
    amount0       NUMERIC,
    amount1       NUMERIC,
    token0        VARCHAR,
    token1        VARCHAR,
    is_b20        BOOLEAN NOT NULL DEFAULT FALSE,
    b20_token     VARCHAR
);

CREATE INDEX IF NOT EXISTS idx_cl_liq_pool   ON aerodrome_cl_liquidity(pool_address);
CREATE INDEX IF NOT EXISTS idx_cl_liq_owner  ON aerodrome_cl_liquidity(owner);
CREATE INDEX IF NOT EXISTS idx_cl_liq_time   ON aerodrome_cl_liquidity(timestamp DESC);

-- ===========================================================================
-- Base B20 tokens
-- ===========================================================================

-- Written from B20Created. Tokens created before the factory emitted that event
-- (which includes the first tokenized stocks) have no row here -- resolve names
-- through b20_token_current instead.
CREATE TABLE IF NOT EXISTS b20_tokens (
    id                VARCHAR PRIMARY KEY,
    address           VARCHAR NOT NULL,
    variant           VARCHAR,
    name              VARCHAR,
    symbol            VARCHAR,
    decimals          VARCHAR,
    created_block     BIGINT,
    created_timestamp BIGINT,
    deployer          VARCHAR
);

CREATE TABLE IF NOT EXISTS b20_events (
    id               VARCHAR PRIMARY KEY,
    tx_hash          VARCHAR NOT NULL,
    log_index        BIGINT  NOT NULL,
    block_number     BIGINT  NOT NULL,
    timestamp        BIGINT  NOT NULL,
    token            VARCHAR NOT NULL,
    variant          VARCHAR,
    event_type       VARCHAR NOT NULL,
    actor            VARCHAR,
    account          VARCHAR,
    recipient        VARCHAR,
    amount           NUMERIC,
    old_value        NUMERIC,
    new_value        NUMERIC,
    effective_at     BIGINT,
    key              VARCHAR,
    text             VARCHAR,
    announcement_id  VARCHAR,
    uri              VARCHAR,
    -- FALSE means the operation ran outside an announcement bracket. The B20
    -- spec asks indexers to surface those as emergency overrides.
    in_announcement  BOOLEAN NOT NULL DEFAULT FALSE,
    pause_features   VARCHAR
);

CREATE INDEX IF NOT EXISTS idx_b20_events_token ON b20_events(token);
CREATE INDEX IF NOT EXISTS idx_b20_events_type  ON b20_events(event_type);
CREATE INDEX IF NOT EXISTS idx_b20_events_time  ON b20_events(timestamp DESC);

-- Corporate actions. `effective_at` may be in the future: the flip is lazy and
-- emits nothing when it happens, so always resolve by timestamp, not by block.
CREATE TABLE IF NOT EXISTS b20_multiplier_schedule (
    id                  VARCHAR PRIMARY KEY,
    token               VARCHAR NOT NULL,
    old_multiplier      NUMERIC,
    new_multiplier      NUMERIC,
    effective_at        BIGINT  NOT NULL,
    announced_block     BIGINT  NOT NULL,
    announced_timestamp BIGINT  NOT NULL,
    tx_hash             VARCHAR NOT NULL,
    log_index           BIGINT  NOT NULL,
    scheduled           BOOLEAN NOT NULL DEFAULT FALSE,
    cancelled           BOOLEAN NOT NULL DEFAULT FALSE,
    announcement_id     VARCHAR
);

CREATE INDEX IF NOT EXISTS idx_b20_mult_token ON b20_multiplier_schedule(token, effective_at DESC);

-- ===========================================================================
-- Views
-- ===========================================================================

-- 2^96, the Q64.96 scale of sqrtPriceX96.
CREATE OR REPLACE VIEW cl_price_constants AS
SELECT 79228162514264337593543950336::NUMERIC AS q96;

-- Current name and symbol per B20, honouring post-launch renames.
-- AAPLc, for instance, was created as `AAPL` and renamed afterwards.
CREATE OR REPLACE VIEW b20_token_current AS
WITH latest_symbol AS (
    SELECT DISTINCT ON (token) token, text AS symbol
    FROM b20_events
    WHERE event_type = 'symbol_updated'
    ORDER BY token, block_number DESC, log_index DESC
),
latest_name AS (
    SELECT DISTINCT ON (token) token, text AS name
    FROM b20_events
    WHERE event_type = 'name_updated'
    ORDER BY token, block_number DESC, log_index DESC
),
seen AS (
    SELECT address AS token, variant, name, symbol FROM b20_tokens
    UNION
    SELECT DISTINCT token, variant, NULL, NULL FROM b20_events
)
SELECT
    s.token,
    MAX(s.variant)                                     AS variant,
    COALESCE(MAX(ln.name),   MAX(s.name))              AS name,
    COALESCE(MAX(ls.symbol), MAX(s.symbol))            AS symbol
FROM seen s
LEFT JOIN latest_symbol ls ON ls.token = s.token
LEFT JOIN latest_name   ln ON ln.token = s.token
GROUP BY s.token;

-- The multiplier in force for a token at a given time.
-- Cancelled schedules are excluded and future-dated ones are ignored until they
-- mature, matching the token's own lazy evaluation. 1e18 means "unchanged".
CREATE OR REPLACE FUNCTION b20_effective_multiplier(p_token VARCHAR, p_at BIGINT)
RETURNS NUMERIC AS $$
    SELECT COALESCE(
        (SELECT new_multiplier
           FROM b20_multiplier_schedule
          WHERE token = p_token
            AND NOT cancelled
            AND effective_at <= p_at
          ORDER BY effective_at DESC, announced_block DESC
          LIMIT 1),
        1000000000000000000::NUMERIC
    );
$$ LANGUAGE SQL STABLE;

-- Latest value of each B20 extra-metadata key. An empty value means the issuer
-- removed the entry, so those are dropped rather than surfaced as blank.
CREATE OR REPLACE VIEW b20_token_metadata AS
SELECT DISTINCT ON (token, key) token, key, text AS value
FROM b20_events
WHERE event_type = 'extra_metadata_updated'
ORDER BY token, key, block_number DESC, log_index DESC;

-- What a B20 token actually is.
--
-- The variant byte only says `asset` vs `stablecoin`, and the Asset variant is
-- explicitly general purpose -- "assets of all kinds" -- so it does NOT mean
-- equity. An ISIN in extra metadata is the real signal: it is a registered
-- securities identifier, and every Coinbase tokenized stock carries one
-- (AAPLc -> US0378331005, NVDAc -> US67066G1040). Tokenized commodities,
-- points or other non-securities will not have one.
CREATE OR REPLACE VIEW b20_token_identity AS
SELECT
    c.token,
    c.variant,
    c.name,
    c.symbol,
    i.value AS isin,
    -- First two ISIN characters are the issuing country.
    SUBSTR(i.value, 1, 2) AS isin_country,
    (i.value IS NOT NULL) AS is_security
FROM b20_token_current c
LEFT JOIN b20_token_metadata i
       ON i.token = c.token AND i.key = 'isin' AND i.value <> '';

-- Every Aerodrome pool holding a B20 token, with the B20 side identified.
CREATE OR REPLACE VIEW b20_pools AS
SELECT
    p.*,
    CASE WHEN p.b20_token = p.token0 THEN p.token1 ELSE p.token0 END AS quote_token,
    CASE WHEN p.b20_token = p.token0 THEN p.token1_symbol ELSE p.token0_symbol END AS quote_symbol,
    CASE WHEN p.b20_token = p.token0 THEN p.token1_decimals ELSE p.token0_decimals END AS quote_decimals,
    CASE WHEN p.b20_token = p.token0 THEN p.token0_decimals ELSE p.token1_decimals END AS b20_decimals,
    COALESCE(c.symbol, CASE WHEN p.b20_token = p.token0 THEN p.token0_symbol ELSE p.token1_symbol END) AS b20_symbol
FROM pools p
LEFT JOIN b20_token_current c ON c.token = p.b20_token
WHERE p.is_b20;

-- Asset-variant B20 pools: tokenized anything (equities, commodities, funds).
CREATE OR REPLACE VIEW b20_asset_pools AS
SELECT * FROM b20_pools WHERE b20_variant = 'asset';

-- Tokenized-SECURITY pools: asset variant AND a registered ISIN.
--
-- Narrower than b20_asset_pools on purpose. Note `createB20` is permissionless,
-- so even an ISIN is self-declared metadata, not proof of issuance -- confirm
-- the issuer before presenting a pool as a given company's stock.
--
-- Requires the token's ExtraMetadataUpdated to have been indexed, which it is
-- on any run starting before the token was created.
CREATE OR REPLACE VIEW stock_pools AS
SELECT bp.*, id.isin, id.isin_country, id.symbol AS security_symbol
FROM b20_pools bp
JOIN b20_token_identity id ON id.token = bp.b20_token
WHERE bp.b20_variant = 'asset' AND id.isin IS NOT NULL;

-- Slipstream swaps with a real price attached.
--
-- Price comes from sqrtPriceX96, which is the pool's post-trade price and is
-- therefore direction-independent -- unlike an output/input ratio, which yields
-- a price one way and its reciprocal the other.
CREATE OR REPLACE VIEW cl_swaps_priced AS
SELECT
    s.*,
    p.pool_type,
    p.token0_symbol,
    p.token1_symbol,
    p.token0_decimals,
    p.token1_decimals,
    CASE
        WHEN p.token0_decimals >= 0 AND p.token1_decimals >= 0
        THEN power(s.sqrt_price_x96 / 79228162514264337593543950336::NUMERIC, 2)
             * power(10::NUMERIC, p.token0_decimals - p.token1_decimals)
    END AS price_token1_per_token0
FROM aerodrome_cl_swaps s
JOIN pools p ON p.address = s.pool_address;

-- Tokenized-stock trades, priced in the quote asset.
--
-- Covers BOTH venues. Tokenized stocks are mostly Slipstream, but real v2 AMM
-- pools hold them too (GOOGLc/AERO and NVDAc/USDC are live v2 pools), and a
-- CL-only view would silently drop those trades.
--
-- `price_raw` is the price of one whole B20 unit. `price_per_share` divides by
-- the multiplier in force, so a stock split shows up as a change in shares per
-- token rather than as a phantom gap in the price series. The multiplier is
-- cosmetic to the AMM -- pool reserves are raw -- so `price_raw` stays
-- continuous across a corporate action by construction.
CREATE OR REPLACE VIEW b20_swaps AS
SELECT
    s.id,
    s.tx_hash,
    s.block_number,
    s.timestamp,
    s.pool_address,
    s.sender,
    s.recipient,
    'cl'::VARCHAR AS venue,
    bp.b20_token,
    bp.b20_symbol,
    bp.quote_symbol,
    CASE WHEN bp.b20_token = s.token0 THEN s.amount0 ELSE s.amount1 END AS b20_amount,
    CASE WHEN bp.b20_token = s.token0 THEN s.amount1 ELSE s.amount0 END AS quote_amount,
    CASE
        WHEN bp.b20_token = s.token0 THEN s.price_token1_per_token0
        WHEN s.price_token1_per_token0 > 0 THEN 1 / s.price_token1_per_token0
    END AS price_raw,
    CASE
        WHEN bp.b20_token = s.token0 THEN s.price_token1_per_token0
        WHEN s.price_token1_per_token0 > 0 THEN 1 / s.price_token1_per_token0
    END / (b20_effective_multiplier(bp.b20_token, s.timestamp) / 1e18) AS price_per_share
FROM cl_swaps_priced s
JOIN b20_pools bp ON bp.address = s.pool_address

UNION ALL

SELECT
    v.id,
    v.tx_hash,
    v.block_number,
    v.timestamp,
    v.pool_address,
    v.sender,
    v.recipient,
    'v2'::VARCHAR AS venue,
    bp.b20_token,
    bp.b20_symbol,
    bp.quote_symbol,
    CASE WHEN bp.b20_token = v.token0
         THEN v.amount0_in - v.amount0_out
         ELSE v.amount1_in - v.amount1_out END AS b20_amount,
    CASE WHEN bp.b20_token = v.token0
         THEN v.amount1_in - v.amount1_out
         ELSE v.amount0_in - v.amount0_out END AS quote_amount,
    -- v2 stores a raw token1-per-token0 ratio; apply decimals and orient it.
    CASE
        WHEN bp.quote_decimals < 0 OR bp.b20_decimals < 0 THEN NULL
        WHEN bp.b20_token = v.token0
            THEN v.price_token1_per_token0 * power(10::NUMERIC, bp.b20_decimals - bp.quote_decimals)
        WHEN v.price_token1_per_token0 > 0
            THEN power(10::NUMERIC, bp.b20_decimals - bp.quote_decimals) / v.price_token1_per_token0
    END AS price_raw,
    CASE
        WHEN bp.quote_decimals < 0 OR bp.b20_decimals < 0 THEN NULL
        WHEN bp.b20_token = v.token0
            THEN v.price_token1_per_token0 * power(10::NUMERIC, bp.b20_decimals - bp.quote_decimals)
        WHEN v.price_token1_per_token0 > 0
            THEN power(10::NUMERIC, bp.b20_decimals - bp.quote_decimals) / v.price_token1_per_token0
    END / (b20_effective_multiplier(bp.b20_token, v.timestamp) / 1e18) AS price_per_share
FROM aerodrome_swaps v
JOIN b20_pools bp ON bp.address = v.pool_address
WHERE v.price_token1_per_token0 IS NOT NULL;

-- Trades in tokenized securities only.
CREATE OR REPLACE VIEW stock_swaps AS
SELECT b.*, id.isin
FROM b20_swaps b
JOIN b20_token_identity id ON id.token = b.b20_token
WHERE id.isin IS NOT NULL;

-- OHLCV for tokenized stocks, derived rather than stored.
-- Pass 300 / 3600 / 14400 / 86400 for 5m / 1h / 4h / 1d.
CREATE OR REPLACE FUNCTION stock_candles(p_interval BIGINT, p_pool VARCHAR DEFAULT NULL)
RETURNS TABLE (
    pool_address VARCHAR,
    b20_symbol   VARCHAR,
    bucket       BIGINT,
    open         NUMERIC,
    high         NUMERIC,
    low          NUMERIC,
    close        NUMERIC,
    volume       NUMERIC,
    trades       BIGINT
) AS $$
    SELECT
        pool_address,
        b20_symbol,
        (timestamp / p_interval) * p_interval AS bucket,
        (array_agg(price_raw ORDER BY block_number,  id))[1] AS open,
        MAX(price_raw)                                       AS high,
        MIN(price_raw)                                       AS low,
        (array_agg(price_raw ORDER BY block_number DESC, id DESC))[1] AS close,
        SUM(ABS(b20_amount))                                 AS volume,
        COUNT(*)                                             AS trades
    FROM stock_swaps
    WHERE price_raw IS NOT NULL
      AND (p_pool IS NULL OR pool_address = p_pool)
    GROUP BY pool_address, b20_symbol, bucket
    ORDER BY bucket DESC;
$$ LANGUAGE SQL STABLE;

-- Rolling 24h activity per tokenized-stock pool.
CREATE OR REPLACE VIEW stock_pools_24h AS
SELECT
    bp.address        AS pool_address,
    bp.b20_symbol,
    bp.quote_symbol,
    bp.tick_spacing,
    COUNT(s.id)                                  AS trades_24h,
    COUNT(DISTINCT s.sender)                     AS traders_24h,
    COALESCE(SUM(ABS(s.b20_amount)), 0)          AS volume_24h,
    MAX(s.price_raw)                             AS high_24h,
    MIN(s.price_raw)                             AS low_24h
FROM stock_pools bp
LEFT JOIN stock_swaps s
       ON s.pool_address = bp.address
      AND s.timestamp > EXTRACT(EPOCH FROM NOW())::BIGINT - 86400
GROUP BY bp.address, bp.b20_symbol, bp.quote_symbol, bp.tick_spacing
ORDER BY volume_24h DESC;

-- Corporate actions, newest first, with the announcement that disclosed them.
CREATE OR REPLACE VIEW b20_corporate_actions AS
SELECT
    m.token,
    c.symbol,
    m.old_multiplier / 1e18 AS old_multiplier,
    m.new_multiplier / 1e18 AS new_multiplier,
    m.effective_at,
    m.announced_timestamp,
    m.scheduled,
    m.cancelled,
    m.announcement_id,
    a.text AS announcement_description,
    a.uri  AS announcement_uri,
    m.effective_at > EXTRACT(EPOCH FROM NOW())::BIGINT AS pending
FROM b20_multiplier_schedule m
LEFT JOIN b20_token_current c ON c.token = m.token
LEFT JOIN b20_events a
       ON a.token = m.token
      AND a.event_type = 'announcement'
      AND a.announcement_id = m.announcement_id
ORDER BY m.effective_at DESC;

-- Operations that changed a token without an announcement bracket around them.
CREATE OR REPLACE VIEW b20_unannounced_actions AS
SELECT * FROM b20_events
WHERE NOT in_announcement
  AND event_type IN (
      'multiplier_updated', 'ui_multiplier_updated', 'ui_multiplier_update_cancelled',
      'paused', 'unpaused', 'seized', 'burned_blocked'
  )
ORDER BY block_number DESC;

-- Trading halts, which the spec expects around a reverse split.
CREATE OR REPLACE VIEW b20_pause_history AS
SELECT token, event_type, pause_features, actor, timestamp, block_number, in_announcement
FROM b20_events
WHERE event_type IN ('paused', 'unpaused')
ORDER BY block_number DESC;

-- ===========================================================================
-- Protocol-wide aggregates (derived)
-- ===========================================================================

CREATE OR REPLACE VIEW pool_stats AS
SELECT
    pool_address,
    COUNT(*)                     AS swap_count,
    SUM(amount_in_total)         AS total_volume,
    MAX(timestamp)               AS last_swap_time
FROM aerodrome_swaps
GROUP BY pool_address;

CREATE OR REPLACE VIEW cl_pool_stats AS
SELECT
    pool_address,
    COUNT(*)                     AS swap_count,
    SUM(ABS(amount0))            AS total_volume_token0,
    SUM(ABS(amount1))            AS total_volume_token1,
    MAX(timestamp)               AS last_swap_time
FROM aerodrome_cl_swaps
GROUP BY pool_address;

CREATE OR REPLACE VIEW trader_stats AS
SELECT
    sender AS wallet_address,
    COUNT(*)             AS total_swaps,
    SUM(amount_in_total) AS total_volume,
    MAX(timestamp)       AS last_swap_time
FROM aerodrome_swaps
GROUP BY sender;

CREATE OR REPLACE VIEW top_pools_24h AS
SELECT pool_address, COUNT(*) AS swaps, SUM(amount_in_total) AS volume
FROM aerodrome_swaps
WHERE timestamp > EXTRACT(EPOCH FROM NOW())::BIGINT - 86400
GROUP BY pool_address
ORDER BY volume DESC;
