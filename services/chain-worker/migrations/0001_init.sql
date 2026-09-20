-- Real projection of IdentityRegistry/AccountRegistry events (ONCHAIN-08,
-- ONCHAIN-09) — a read cache of on-chain state, never a second source of
-- truth. `tracks` exists because AccountRegistry's events key accounts by
-- trackId, not identityId directly, matching the real contract shape
-- (identity -> track -> account), not a simplification of it.
CREATE TABLE indexed_blocks (
    number BIGINT PRIMARY KEY,
    hash TEXT NOT NULL,
    parent_hash TEXT NOT NULL,
    tag TEXT NOT NULL CHECK (tag IN ('included', 'safe', 'finalized'))
);

CREATE TABLE identities (
    identity_id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    pending_owner TEXT,
    owner_epoch BIGINT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending_confirmation', 'active')),
    created_at_block BIGINT NOT NULL REFERENCES indexed_blocks(number) ON DELETE CASCADE
);

CREATE TABLE tracks (
    track_id TEXT PRIMARY KEY,
    identity_id TEXT NOT NULL REFERENCES identities(identity_id) ON DELETE CASCADE,
    financial_profile_id TEXT NOT NULL,
    denomination_commitment TEXT NOT NULL,
    created_at_block BIGINT NOT NULL REFERENCES indexed_blocks(number) ON DELETE CASCADE
);

CREATE TABLE accounts (
    account_id TEXT PRIMARY KEY,
    track_id TEXT NOT NULL REFERENCES tracks(track_id) ON DELETE CASCADE,
    venue_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending_baseline', 'active', 'removed')),
    created_at_block BIGINT NOT NULL REFERENCES indexed_blocks(number) ON DELETE CASCADE
);
