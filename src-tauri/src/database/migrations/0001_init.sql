-- Mac Observer schema v1
-- All timestamps are Unix epoch milliseconds (UTC).
-- API keys are NEVER stored in this database (see security/keychain.rs).

CREATE TABLE IF NOT EXISTS application_settings (
    key         TEXT PRIMARY KEY,
    value_json  TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- Downsampled system metrics (one row per persistence interval, default 10 s).
CREATE TABLE IF NOT EXISTS system_metrics (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    ts               INTEGER NOT NULL,
    cpu_usage        REAL,
    cpu_user         REAL,
    cpu_system       REAL,
    mem_used         INTEGER,
    mem_total        INTEGER,
    mem_pressure     INTEGER,
    swap_used        INTEGER,
    gpu_usage        REAL,
    disk_read_bps    REAL,
    disk_write_bps   REAL,
    net_rx_bps       REAL,
    net_tx_bps       REAL,
    battery_percent  REAL,
    battery_charging INTEGER,
    cpu_temp         REAL
);
CREATE INDEX IF NOT EXISTS idx_system_metrics_ts ON system_metrics(ts);

CREATE TABLE IF NOT EXISTS ai_providers (
    id                     TEXT PRIMARY KEY,          -- provider kind, e.g. 'openai'
    display_name           TEXT NOT NULL,
    enabled                INTEGER NOT NULL DEFAULT 0,
    base_url               TEXT,                      -- only for custom providers
    sync_interval_minutes  INTEGER NOT NULL DEFAULT 60,
    selected_models_json   TEXT NOT NULL DEFAULT '[]',
    last_sync_at           INTEGER,
    last_sync_status       TEXT,                      -- ok | error | unsupported
    last_error             TEXT,
    account_json           TEXT,                      -- balance / credit info reported by provider
    created_at             INTEGER NOT NULL,
    updated_at             INTEGER NOT NULL
);

-- Pricing table used ONLY for estimated cost. Prices are USD per 1M tokens.
CREATE TABLE IF NOT EXISTS ai_models (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    provider              TEXT NOT NULL,
    model                 TEXT NOT NULL,
    input_price           REAL,
    output_price          REAL,
    cached_input_price    REAL,
    cache_write_price     REAL,
    price_source          TEXT NOT NULL,              -- e.g. 'user' or 'anthropic-pricing@2026-11-03'
    updated_at            INTEGER NOT NULL,
    UNIQUE(provider, model)
);

CREATE TABLE IF NOT EXISTS ai_usage_records (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    provider            TEXT NOT NULL,
    model               TEXT NOT NULL,
    source              TEXT NOT NULL CHECK (source IN ('provider_reported','telemetry','imported')),
    bucket_start        INTEGER NOT NULL,
    bucket_end          INTEGER NOT NULL,
    input_tokens        INTEGER NOT NULL DEFAULT 0,  -- uncached input
    output_tokens       INTEGER NOT NULL DEFAULT 0,
    cached_tokens       INTEGER NOT NULL DEFAULT 0,  -- cache reads
    cache_write_tokens  INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens    INTEGER NOT NULL DEFAULT 0,  -- informational, included in output_tokens
    requests            INTEGER NOT NULL DEFAULT 0,
    cost_usd            REAL,                        -- only when reported by provider/response
    cost_source         TEXT,                        -- 'response' | 'imported'
    external_id         TEXT NOT NULL,
    created_at          INTEGER NOT NULL,
    UNIQUE(provider, source, external_id)
);
CREATE INDEX IF NOT EXISTS idx_usage_bucket ON ai_usage_records(bucket_start);
CREATE INDEX IF NOT EXISTS idx_usage_provider_bucket ON ai_usage_records(provider, bucket_start);

CREATE TABLE IF NOT EXISTS ai_cost_records (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    provider      TEXT NOT NULL,
    model         TEXT,
    source        TEXT NOT NULL CHECK (source IN ('provider_reported','imported')),
    bucket_start  INTEGER NOT NULL,
    bucket_end    INTEGER NOT NULL,
    amount_usd    REAL NOT NULL,
    line_item     TEXT,
    external_id   TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    UNIQUE(provider, source, external_id)
);
CREATE INDEX IF NOT EXISTS idx_cost_bucket ON ai_cost_records(bucket_start);

CREATE TABLE IF NOT EXISTS alert_rules (
    id          TEXT PRIMARY KEY,                    -- rule kind, e.g. 'cpu_high'
    category    TEXT NOT NULL CHECK (category IN ('system','ai')),
    enabled     INTEGER NOT NULL DEFAULT 1,
    threshold   REAL NOT NULL,
    duration_s  INTEGER NOT NULL DEFAULT 0,          -- condition must hold this long
    cooldown_s  INTEGER NOT NULL DEFAULT 900,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS alert_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id    TEXT NOT NULL,
    severity   TEXT NOT NULL CHECK (severity IN ('info','warning','critical')),
    title      TEXT NOT NULL,
    message    TEXT NOT NULL,
    value      REAL,
    ts         INTEGER NOT NULL,
    notified   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_alert_events_ts ON alert_events(ts);
