-- Binance connection storage (follow-up: real multi-user product,
-- not a one-off homologation script). One row per account's connected
-- exchange credential; encrypted at rest (see src/crypto.rs).
CREATE TABLE binance_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id TEXT NOT NULL UNIQUE,
    encrypted_api_key BYTEA NOT NULL,
    nonce_api_key BYTEA NOT NULL,
    encrypted_api_secret BYTEA NOT NULL,
    nonce_api_secret BYTEA NOT NULL,
    -- Symbols this connection tracks trades for. myTrades requires a
    -- symbol per call (Binance has no "all symbols" trade history
    -- endpoint), so this is an explicit, product-level choice rather
    -- than an attempted auto-discovery of every symbol ever traded.
    symbols TEXT[] NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_synced_at TIMESTAMPTZ
);

-- Namespaced (symbol, trade_id), matching binance-trades' own
-- namespace rule: the same id under two different symbols is never a
-- collision.
CREATE TABLE binance_synced_trades (
    connection_id UUID NOT NULL REFERENCES binance_connections(id) ON DELETE CASCADE,
    symbol TEXT NOT NULL,
    trade_id BIGINT NOT NULL,
    order_id BIGINT NOT NULL,
    price TEXT NOT NULL,
    qty TEXT NOT NULL,
    commission TEXT NOT NULL,
    commission_asset TEXT NOT NULL,
    time_ms BIGINT NOT NULL,
    is_buyer BOOLEAN NOT NULL,
    PRIMARY KEY (connection_id, symbol, trade_id)
);

-- One row per binance_flows::NormalizedFlow, deduplicated by the same
-- (source_namespace, source_id) the domain type already carries.
CREATE TABLE binance_synced_flows (
    connection_id UUID NOT NULL REFERENCES binance_connections(id) ON DELETE CASCADE,
    source_namespace TEXT NOT NULL,
    source_id TEXT NOT NULL,
    economic_time_ms BIGINT NOT NULL,
    kind TEXT NOT NULL,
    legs_json JSONB NOT NULL,
    fee_json JSONB,
    PRIMARY KEY (connection_id, source_namespace, source_id)
);

CREATE TABLE binance_synced_catalog_symbols (
    connection_id UUID NOT NULL REFERENCES binance_connections(id) ON DELETE CASCADE,
    captured_at_ms BIGINT NOT NULL,
    symbol TEXT NOT NULL,
    PRIMARY KEY (connection_id, captured_at_ms, symbol)
);

CREATE INDEX idx_binance_connections_status_sync ON binance_connections (status, last_synced_at);
