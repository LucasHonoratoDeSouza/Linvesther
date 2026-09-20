// Real ProjectionStore (services/chain-worker/src/indexer/types.ts),
// backed by Postgres, plus a domain-specific read API apps/api actually
// calls. The generic Indexer/ChainClient interfaces have no concept of
// event logs (by design — they're reused for any future indexer, not
// just this one), so `apply(block)` independently fetches this block's
// IdentityRegistry/AccountRegistry logs via its own viem PublicClient
// and applies each decoded domain event, all within the same call.
import type { Log, PublicClient } from "viem";
import type { Pool } from "pg";
import type { IndexedBlock, ProjectionStore } from "../indexer/types.js";
import { decodeDomainEvent, type DomainEvent } from "./events.js";

export interface Identity {
  identityId: `0x${string}`;
  owner: `0x${string}`;
  pendingOwner: `0x${string}` | null;
  ownerEpoch: bigint;
  state: "pending_confirmation" | "active";
}

export interface Account {
  accountId: `0x${string}`;
  trackId: `0x${string}`;
  venueId: `0x${string}`;
  state: "pending_baseline" | "active" | "removed";
}

export class PostgresProjectionStore implements ProjectionStore {
  constructor(
    private readonly pool: Pool,
    private readonly publicClient: PublicClient,
    private readonly watchedAddresses: `0x${string}`[],
  ) {}

  async apply(block: IndexedBlock): Promise<void> {
    await this.pool.query(
      `INSERT INTO indexed_blocks (number, hash, parent_hash, tag) VALUES ($1, $2, $3, $4)
       ON CONFLICT (number) DO UPDATE SET hash = EXCLUDED.hash, parent_hash = EXCLUDED.parent_hash, tag = EXCLUDED.tag`,
      [block.header.number.toString(), block.header.hash, block.header.parentHash, block.tag],
    );

    const logs: Log[] = await this.publicClient.getLogs({
      address: this.watchedAddresses,
      fromBlock: block.header.number,
      toBlock: block.header.number,
    });
    for (const log of logs) {
      const event = decodeDomainEvent(log);
      if (event) {
        await this.applyEvent(event, block.header.number);
      }
    }
  }

  /** The highest block number persisted so far, or `null` if nothing has
   * ever been indexed. The `Indexer` itself keeps no state across
   * process restarts (its `chain` is in-memory only) — this is how a
   * fresh `Indexer` instance can resume from where a previous process
   * left off instead of re-walking from the contracts' deploy block
   * every time. */
  async latestIndexedBlock(): Promise<bigint | null> {
    const result = await this.pool.query<{ max: string | null }>("SELECT max(number) FROM indexed_blocks");
    const max = result.rows[0]?.max;
    return max === null || max === undefined ? null : BigInt(max);
  }

  async revert(block: IndexedBlock): Promise<void> {
    // Deletes cascade to any identity/track/account row created at this
    // block (see migrations/0001_init.sql's foreign keys) — a reverted
    // block's projection is undone in full, not left half-applied.
    await this.pool.query("DELETE FROM indexed_blocks WHERE number = $1", [block.header.number.toString()]);
  }

  /** Turns one decoded domain event (events.ts) into the corresponding
   * row insert/update, at `blockNumber` (must already be `apply`'d). */
  async applyEvent(event: DomainEvent, blockNumber: bigint): Promise<void> {
    switch (event.name) {
      case "IdentityCreated":
        await this.pool.query(
          `INSERT INTO identities (identity_id, owner, pending_owner, owner_epoch, state, created_at_block)
           VALUES ($1, $2, NULL, 0, 'pending_confirmation', $3)
           ON CONFLICT (identity_id) DO NOTHING`,
          [event.identityId, event.owner, blockNumber.toString()],
        );
        return;
      case "TrackCreated":
        await this.pool.query(
          `INSERT INTO tracks (track_id, identity_id, financial_profile_id, denomination_commitment, created_at_block)
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (track_id) DO NOTHING`,
          [event.trackId, event.identityId, event.financialProfileId, event.denominationCommitment, blockNumber.toString()],
        );
        return;
      case "OwnerRotationProposed":
        await this.pool.query("UPDATE identities SET pending_owner = $1 WHERE identity_id = $2", [
          event.pendingOwner,
          event.identityId,
        ]);
        return;
      case "OwnerRotated":
        await this.pool.query(
          `UPDATE identities SET owner = $1, pending_owner = NULL, owner_epoch = $2, state = 'active' WHERE identity_id = $3`,
          [event.newOwner, event.newOwnerEpoch.toString(), event.identityId],
        );
        return;
      case "AccountRegistered":
        await this.pool.query(
          `INSERT INTO accounts (account_id, track_id, venue_id, state, created_at_block)
           VALUES ($1, $2, $3, 'pending_baseline', $4)
           ON CONFLICT (account_id) DO NOTHING`,
          [event.accountId, event.trackId, event.venueId, blockNumber.toString()],
        );
        return;
      case "AccountActivated":
        await this.pool.query("UPDATE accounts SET state = 'active' WHERE account_id = $1", [event.accountId]);
        return;
      case "AccountRemoved":
        await this.pool.query("UPDATE accounts SET state = 'removed' WHERE account_id = $1", [event.accountId]);
        return;
    }
  }

  async getIdentity(identityId: `0x${string}`): Promise<Identity | null> {
    const { rows } = await this.pool.query(
      "SELECT identity_id, owner, pending_owner, owner_epoch, state FROM identities WHERE identity_id = $1",
      [identityId],
    );
    const row = rows[0] as
      | { identity_id: string; owner: string; pending_owner: string | null; owner_epoch: string; state: string }
      | undefined;
    if (!row) return null;
    return {
      identityId: row.identity_id as `0x${string}`,
      owner: row.owner as `0x${string}`,
      pendingOwner: row.pending_owner as `0x${string}` | null,
      ownerEpoch: BigInt(row.owner_epoch),
      state: row.state as Identity["state"],
    };
  }

  /** The identity whose current owner is `owner`, if any — an owner has at most one. */
  async getIdentityByOwner(owner: `0x${string}`): Promise<Identity | null> {
    const { rows } = await this.pool.query("SELECT identity_id FROM identities WHERE lower(owner) = lower($1) ORDER BY created_at_block LIMIT 1", [owner]);
    const row = rows[0] as { identity_id: string } | undefined;
    return row ? this.getIdentity(row.identity_id as `0x${string}`) : null;
  }

  async getAccount(accountId: `0x${string}`): Promise<Account | null> {
    const { rows } = await this.pool.query("SELECT account_id, track_id, venue_id, state FROM accounts WHERE account_id = $1", [
      accountId,
    ]);
    const row = rows[0] as { account_id: string; track_id: string; venue_id: string; state: string } | undefined;
    if (!row) return null;
    return {
      accountId: row.account_id as `0x${string}`,
      trackId: row.track_id as `0x${string}`,
      venueId: row.venue_id as `0x${string}`,
      state: row.state as Account["state"],
    };
  }
}
