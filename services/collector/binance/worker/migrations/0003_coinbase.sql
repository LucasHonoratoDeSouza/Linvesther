-- Coinbase (Advanced Trade) connections, alongside the Binance ones. The
-- API key name and private key are encrypted at rest exactly like a
-- Binance key/secret (see src/crypto.rs).
CREATE TABLE coinbase_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id TEXT NOT NULL UNIQUE,
    encrypted_key_name BYTEA NOT NULL,
    nonce_key_name BYTEA NOT NULL,
    encrypted_private_key BYTEA NOT NULL,
    nonce_private_key BYTEA NOT NULL,
    label TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_synced_at TIMESTAMPTZ
);

-- Every fill since the connection. `reconciled` marks the ones already
-- accounted for in a balance snapshot (see src/coinbase.rs).
CREATE TABLE coinbase_fills (
    connection_id UUID NOT NULL REFERENCES coinbase_connections(id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL,
    trade_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    product_id TEXT NOT NULL,
    is_buy BOOLEAN NOT NULL,
    price TEXT NOT NULL,
    size TEXT NOT NULL,
    size_in_quote BOOLEAN NOT NULL,
    commission TEXT NOT NULL,
    time_ms BIGINT NOT NULL,
    reconciled BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (connection_id, entry_id)
);

-- The account's total balance per asset at a moment, as {"BTC": "0.6", ...}.
CREATE TABLE coinbase_balance_snapshots (
    connection_id UUID NOT NULL REFERENCES coinbase_connections(id) ON DELETE CASCADE,
    taken_at_ms BIGINT NOT NULL,
    balances JSONB NOT NULL,
    PRIMARY KEY (connection_id, taken_at_ms)
);

-- Money that came in or left without a trade explaining it: a deposit
-- (positive) or a withdrawal (negative), found by comparing consecutive
-- balance snapshots against the fills between them.
CREATE TABLE coinbase_inferred_flows (
    connection_id UUID NOT NULL REFERENCES coinbase_connections(id) ON DELETE CASCADE,
    at_ms BIGINT NOT NULL,
    asset TEXT NOT NULL,
    amount TEXT NOT NULL,
    PRIMARY KEY (connection_id, at_ms, asset)
);
