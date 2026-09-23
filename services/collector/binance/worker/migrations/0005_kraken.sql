-- Kraken (spot) connections, alongside the Binance, Coinbase and Interactive
-- Brokers ones. The API key and its secret are encrypted at rest exactly like a
-- Binance key/secret (see src/crypto.rs).
CREATE TABLE kraken_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id TEXT NOT NULL UNIQUE,
    encrypted_api_key BYTEA NOT NULL,
    nonce_api_key BYTEA NOT NULL,
    encrypted_api_secret BYTEA NOT NULL,
    nonce_api_secret BYTEA NOT NULL,
    label TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_synced_at TIMESTAMPTZ,
    -- When the key was last confirmed unable to place orders or withdraw. Kraken
    -- limits how often a key may be used, so this is re-checked now and then
    -- rather than on every request (see src/kraken.rs).
    verified_read_only_at TIMESTAMPTZ
);

-- Every trade since the connection, in Linvesther's names for the market and
-- its assets (`BTC-USD`, `BTC`, `USD`). `volume` is in base units and `fee` is
-- charged in the quote currency.
CREATE TABLE kraken_trades (
    connection_id UUID NOT NULL REFERENCES kraken_connections(id) ON DELETE CASCADE,
    trade_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    base TEXT NOT NULL,
    quote TEXT NOT NULL,
    is_buy BOOLEAN NOT NULL,
    price TEXT NOT NULL,
    volume TEXT NOT NULL,
    fee TEXT NOT NULL,
    time_ms BIGINT NOT NULL,
    PRIMARY KEY (connection_id, trade_id)
);

-- Every line of the account ledger since the connection: each change to a
-- balance, whatever caused it. Deposits and withdrawals, staking rewards and
-- conversions are read from here, classified when read (see src/kraken.rs).
CREATE TABLE kraken_ledger (
    connection_id UUID NOT NULL REFERENCES kraken_connections(id) ON DELETE CASCADE,
    ledger_id TEXT NOT NULL,
    refid TEXT NOT NULL,
    time_ms BIGINT NOT NULL,
    entry_type TEXT NOT NULL,
    subtype TEXT NOT NULL,
    asset TEXT NOT NULL,
    amount TEXT NOT NULL,
    fee TEXT NOT NULL,
    PRIMARY KEY (connection_id, ledger_id)
);
