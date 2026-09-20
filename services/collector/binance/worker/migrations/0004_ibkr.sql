-- Interactive Brokers (Flex Web Service) connections, alongside Binance
-- and Coinbase. Both the token and the Flex Query ID are read-only and
-- encrypted at rest exactly like the other exchanges' credentials.
CREATE TABLE ibkr_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id TEXT NOT NULL UNIQUE,
    encrypted_token BYTEA NOT NULL,
    nonce_token BYTEA NOT NULL,
    encrypted_query_id BYTEA NOT NULL,
    nonce_query_id BYTEA NOT NULL,
    label TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_synced_at TIMESTAMPTZ
);

-- One row per report date the Flex report has given a total account
-- value for — IBKR reports this once a day, already net of that day's
-- trades and flows, so no price lookups are needed to value it (unlike
-- the crypto exchanges, whose engine derives NAV from trades+market
-- data instead).
CREATE TABLE ibkr_equity_points (
    connection_id UUID NOT NULL REFERENCES ibkr_connections(id) ON DELETE CASCADE,
    report_date DATE NOT NULL,
    total NUMERIC NOT NULL,
    PRIMARY KEY (connection_id, report_date)
);

-- Deposits/withdrawals only (IBKR's own "Deposits/Withdrawals" cash
-- transaction type) — dividends, interest and fees are excluded so they
-- are not mistaken for capital moving in or out.
CREATE TABLE ibkr_flows (
    connection_id UUID NOT NULL REFERENCES ibkr_connections(id) ON DELETE CASCADE,
    report_date DATE NOT NULL,
    amount NUMERIC NOT NULL,
    currency TEXT NOT NULL,
    PRIMARY KEY (connection_id, report_date, amount)
);

-- Trades, kept only for the win-rate metric — no per-position detail
-- (average cost, unrealized P&L) is computed for IBKR accounts yet.
CREATE TABLE ibkr_trades (
    connection_id UUID NOT NULL REFERENCES ibkr_connections(id) ON DELETE CASCADE,
    trade_date DATE NOT NULL,
    symbol TEXT NOT NULL,
    is_buy BOOLEAN NOT NULL,
    quantity NUMERIC NOT NULL,
    price NUMERIC NOT NULL,
    ordinal INT NOT NULL,
    PRIMARY KEY (connection_id, trade_date, symbol, ordinal)
);
