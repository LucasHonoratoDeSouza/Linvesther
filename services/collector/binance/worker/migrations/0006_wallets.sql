-- On-chain wallets (EVM), alongside the exchange and broker connections. A wallet has no
-- credential: what is stored is its address, encrypted at rest and bound to the account exactly
-- like a credential (see src/crypto.rs), because the address is what links a person to
-- everything the wallet did on a public chain.
CREATE TABLE wallet_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id TEXT NOT NULL UNIQUE,
    -- The identity that connected it, lower case. The same address may be connected by
    -- different owners; one owner does not connect it twice (checked when connecting).
    owner_address TEXT NOT NULL,
    encrypted_address BYTEA NOT NULL,
    nonce_address BYTEA NOT NULL,
    label TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_synced_at TIMESTAMPTZ
);

-- One row per network the connection reads. The set is fixed when the wallet is connected:
-- a network switched on later would bring balances with no history behind them.
-- `synced_block` is the newest block whose transfers are kept.
CREATE TABLE wallet_chain_state (
    connection_id UUID NOT NULL REFERENCES wallet_connections(id) ON DELETE CASCADE,
    chain_id BIGINT NOT NULL,
    synced_block BIGINT NOT NULL,
    synced_at_ms BIGINT NOT NULL,
    -- The explorer warned that some transfers of the last range may be missing.
    transfers_incomplete BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (connection_id, chain_id)
);

-- What each transaction did to a balance since the connection, in the asset's own unit.
-- `unexplained` rows are differences between balances that the transfers do not explain.
CREATE TABLE wallet_effects (
    connection_id UUID NOT NULL REFERENCES wallet_connections(id) ON DELETE CASCADE,
    chain_id BIGINT NOT NULL,
    tx_hash TEXT NOT NULL,
    asset TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('transfer', 'gas', 'unexplained')),
    block BIGINT NOT NULL,
    time_ms BIGINT NOT NULL,
    delta TEXT NOT NULL,
    PRIMARY KEY (connection_id, chain_id, tx_hash, asset, kind)
);

-- The balance of each followed asset as of `block` on its network, the point the history is
-- rebuilt backwards from. An asset whose amount cannot be held exactly is marked.
CREATE TABLE wallet_anchors (
    connection_id UUID NOT NULL REFERENCES wallet_connections(id) ON DELETE CASCADE,
    chain_id BIGINT NOT NULL,
    asset TEXT NOT NULL,
    quantity TEXT NOT NULL,
    block BIGINT NOT NULL,
    symbol TEXT NOT NULL DEFAULT '',
    decimals INTEGER NOT NULL DEFAULT 18,
    unrepresentable BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (connection_id, chain_id, asset)
);
