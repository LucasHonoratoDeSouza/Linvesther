// Real identity/account lifecycle, replacing the in-memory simulation
// in commands.ts/store.ts: every state change here is a real relayed
// transaction against the deployed IdentityRegistry/AccountRegistry,
// read back through a real Postgres projection kept current by the
// real Indexer.
//
// Limitation: only the vault credential method can confirm an
// on-chain rotation right now. A WebAuthn confirmation needs its
// browser assertion re-encoded into WebAuthnAccount's ABI-encoded
// WebAuthnAuth wire format (authenticatorData/clientDataJSON/r/s) —
// nothing in the codebase does that encoding yet (only the reverse:
// finishAuthentication's off-chain parse/verify). Rather than ship a
// half-working encoder, a WebAuthn confirmation attempt fails with a
// clear, typed error instead of a wrong signature reaching the chain.
import type { Indexer, ChainClient, Account as ChainAccount } from "@linvestherzk/chain-worker";
import {
  accountFactoryAbi,
  accountRegistryAbi,
  identityRegistryAbi,
  buildConfirmOwnerRotationChallenge,
  confirmIdentityCreation as relayConfirmIdentityCreation,
  startIdentityCreation,
  buildCreateTrackChallenge,
  createTrack as relayCreateTrack,
  buildRegisterAccountChallenge,
  registerAccount as relayRegisterAccount,
  buildActivateAccountChallenge,
  activateAccount as relayActivateAccount,
  buildRemoveAccountChallenge,
  buildSetProfileChallenge,
  setProfile as relaySetProfile,
  readProfile,
  removeAccount as relayRemoveAccount,
  type ConfirmOwnerRotationChallenge,
  type CreateTrackChallenge,
  type RegisterAccountChallenge,
  type ActivateAccountChallenge,
  type RemoveAccountChallenge,
  type SetProfileChallenge,
  type OnChainProfile,
  type PostgresProjectionStore,
  type RelayFlowDeps,
} from "@linvestherzk/chain-worker";
import type { Credential, CredentialStore } from "../auth/credentialStore.js";
import { LifecycleError } from "./types.js";

export interface ChainLifecycleDeps {
  relayFlowDeps: RelayFlowDeps;
  projectionStore: PostgresProjectionStore;
  indexer: Indexer;
  chainClient: ChainClient;
  credentialStore: CredentialStore;
  /** Where the optional public name/bio lives; profiles are unavailable when unset. */
  profileRegistry?: `0x${string}`;
  /** Where the very first sync after this process started should begin
   * walking from — the indexer's own view starts empty on every restart
   * (Indexer.chain is in-memory only), so this is its cold-start point.
   * server.ts computes it as the highest block a previous process
   * already persisted to Postgres, falling back to the registry
   * contracts' actual deploy block only on a genuinely first-ever run —
   * never 0n on a real, long-lived chain, whose public RPC may not even
   * serve history that far back (pruned nodes). */
  deployBlock: bigint;
}

async function resync(deps: ChainLifecycleDeps): Promise<void> {
  await deps.indexer.sync(deps.chainClient, deps.deployBlock);
}

/** Resyncs, then retries `read` until it returns a non-null result —
 * up to a few seconds. A load-balanced public RPC (e.g. Base Sepolia's
 * shared endpoint) can serve `getSnapshot()` from a node that hasn't
 * yet caught up to a transaction this same request just had mined
 * moments earlier (the same node-lag behavior ViemBroadcastClient's
 * own doc comment describes for gas estimation) — a single resync+read
 * can race that lag and see pre-change state. Each retry gives the
 * RPC's node pool another moment to converge; `read`'s own eventual
 * `null` (after all retries) still surfaces as the same real error the
 * call site already throws today. */
async function resyncUntil<T>(deps: ChainLifecycleDeps, read: () => Promise<T | null>): Promise<T | null> {
  const delaysMs = [0, 500, 1000, 1500, 2000, 2500];
  let result: T | null = null;
  for (const delayMs of delaysMs) {
    if (delayMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, delayMs));
    }
    await resync(deps);
    result = await read();
    if (result) return result;
  }
  return result;
}

async function requireCredential(deps: ChainLifecycleDeps, subjectKey: `0x${string}`): Promise<Credential> {
  const credential = await deps.credentialStore.get(subjectKey);
  if (!credential) {
    throw new LifecycleError("not_found", `no credential registered for ${subjectKey}`);
  }
  return credential;
}

async function predictedAccountAddress(deps: ChainLifecycleDeps, credential: Credential): Promise<`0x${string}`> {
  const predictFn = credential.method === "webauthn" ? "predictWebAuthnAccountAddress" : "predictP256VaultAccountAddress";
  return (await deps.relayFlowDeps.publicClient.readContract({
    address: deps.relayFlowDeps.accountFactory,
    abi: accountFactoryAbi,
    functionName: predictFn,
    args: [credential.qx, credential.qy],
  })) as `0x${string}`;
}

/** Throws unless `subjectKey`'s own registered credential controls
 * `identityId`'s current real owner — the check every track/account
 * command needs, since by the time a track exists the identity is past
 * the relayer-as-temporary-owner window and has a real owner already. */
async function requireIdentityOwner(deps: ChainLifecycleDeps, identityId: `0x${string}`, subjectKey: `0x${string}`): Promise<Credential> {
  const credential = await requireCredential(deps, subjectKey);
  const identity = await deps.projectionStore.getIdentity(identityId);
  if (!identity) {
    throw new LifecycleError("not_found", `identity ${identityId} does not exist`);
  }
  const predicted = await predictedAccountAddress(deps, credential);
  if (predicted.toLowerCase() !== identity.owner.toLowerCase()) {
    throw new LifecycleError("forbidden", `${subjectKey} does not own identity ${identityId}`);
  }
  return credential;
}

/** Starts a real on-chain identity for the session's registered
 * credential: deploys its account (if needed), creates the identity
 * (the relayer is its temporary owner), and proposes the account as
 * the real owner. Returns the identity in `pending_confirmation` —
 * never `active` from this call alone, per ONCHAIN-07. */
export async function createIdentity(deps: ChainLifecycleDeps, subjectKey: `0x${string}`) {
  const credential = await requireCredential(deps, subjectKey);
  const { identityId } = await startIdentityCreation(deps.relayFlowDeps, credential.qx, credential.qy, credential.method);
  // Waits for pendingOwner specifically, not just the identity row's
  // existence — IdentityCreated alone (with no pendingOwner yet) can
  // already be indexed while the very next proposeOwnerRotation event,
  // from a transaction this same call also already waited on being
  // mined, hasn't reached this projection's view yet.
  const identity = await resyncUntil(deps, async () => {
    const found = await deps.projectionStore.getIdentity(identityId);
    return found?.pendingOwner ? found : null;
  });
  if (!identity) {
    throw new Error(`identity ${identityId} not found in projection immediately after creation`);
  }
  return identity;
}

/** Builds the exact EIP-712 challenge the session's own credential must
 * sign to confirm `identityId` — only the identity's actual
 * `pendingOwner` may be confirmed, verified by recomputing that
 * pending owner's predicted address from the requester's own
 * registered public key (never trusting a caller-supplied address). */
export async function beginConfirmIdentity(
  deps: ChainLifecycleDeps,
  identityId: `0x${string}`,
  subjectKey: `0x${string}`,
): Promise<ConfirmOwnerRotationChallenge> {
  const credential = await requireCredential(deps, subjectKey);
  const identity = await deps.projectionStore.getIdentity(identityId);
  if (!identity) {
    throw new LifecycleError("not_found", `identity ${identityId} does not exist`);
  }
  if (!identity.pendingOwner) {
    throw new LifecycleError("invalid_state", `identity ${identityId} has no pending rotation to confirm`);
  }

  const predicted = await predictedAccountAddress(deps, credential);
  if (predicted.toLowerCase() !== identity.pendingOwner.toLowerCase()) {
    throw new LifecycleError("forbidden", `${subjectKey} does not control the pending owner of identity ${identityId}`);
  }

  return buildConfirmOwnerRotationChallenge(deps.relayFlowDeps, identityId, identity.pendingOwner);
}

export class UnsupportedConfirmationMethodError extends Error {
  constructor() {
    super("On-chain confirmation is only supported for the vault credential method right now.");
  }
}

/** Relays the session's confirmation signature over `challenge` —
 * `challenge` must be exactly what `beginConfirmIdentity` returned. */
export async function confirmIdentity(
  deps: ChainLifecycleDeps,
  subjectKey: `0x${string}`,
  challenge: ConfirmOwnerRotationChallenge,
  signature: `0x${string}`,
) {
  const credential = await requireCredential(deps, subjectKey);
  if (credential.method !== "vault") {
    throw new UnsupportedConfirmationMethodError();
  }

  await relayConfirmIdentityCreation(deps.relayFlowDeps, challenge, signature);
  const identity = await resyncUntil(deps, async () => {
    const found = await deps.projectionStore.getIdentity(challenge.message.identityId);
    return found?.state === "active" ? found : null;
  });
  if (!identity) {
    throw new Error(`identity ${challenge.message.identityId} not found in projection immediately after confirmation`);
  }
  return identity;
}

/** Builds the EIP-712 challenge to create a track under `identityId` —
 * only that identity's real owner may request it. */
export async function beginCreateTrack(
  deps: ChainLifecycleDeps,
  identityId: `0x${string}`,
  subjectKey: `0x${string}`,
  financialProfileId: `0x${string}`,
  denominationCommitment: `0x${string}`,
): Promise<CreateTrackChallenge> {
  await requireIdentityOwner(deps, identityId, subjectKey);
  return buildCreateTrackChallenge(deps.relayFlowDeps, identityId, financialProfileId, denominationCommitment);
}

export async function createTrack(deps: ChainLifecycleDeps, subjectKey: `0x${string}`, challenge: CreateTrackChallenge, signature: `0x${string}`) {
  const credential = await requireCredential(deps, subjectKey);
  if (credential.method !== "vault") {
    throw new UnsupportedConfirmationMethodError();
  }
  const result = await relayCreateTrack(deps.relayFlowDeps, challenge, signature);
  await resync(deps);
  return result;
}

async function requireTrackIdentityOwner(deps: ChainLifecycleDeps, trackId: `0x${string}`, subjectKey: `0x${string}`): Promise<Credential> {
  const credential = await requireCredential(deps, subjectKey);
  const [identityId] = (await deps.relayFlowDeps.publicClient.readContract({
    address: deps.relayFlowDeps.identityRegistry,
    abi: identityRegistryAbi,
    functionName: "tracks",
    args: [trackId],
  })) as readonly [`0x${string}`, ...unknown[]];
  const identity = await deps.projectionStore.getIdentity(identityId);
  if (!identity) {
    throw new LifecycleError("not_found", `track ${trackId} does not belong to a known identity`);
  }
  const predicted = await predictedAccountAddress(deps, credential);
  if (predicted.toLowerCase() !== identity.owner.toLowerCase()) {
    throw new LifecycleError("forbidden", `${subjectKey} does not own the identity behind track ${trackId}`);
  }
  return credential;
}

/** Builds the EIP-712 challenge to register an account under `trackId`
 * — only that track's identity owner may request it. */
export async function beginRegisterAccount(
  deps: ChainLifecycleDeps,
  trackId: `0x${string}`,
  subjectKey: `0x${string}`,
  venueId: `0x${string}`,
  authenticatedIdCommitment: `0x${string}`,
  environment: number,
): Promise<RegisterAccountChallenge> {
  await requireTrackIdentityOwner(deps, trackId, subjectKey);
  return buildRegisterAccountChallenge(deps.relayFlowDeps, trackId, venueId, authenticatedIdCommitment, environment);
}

export async function registerAccount(deps: ChainLifecycleDeps, subjectKey: `0x${string}`, challenge: RegisterAccountChallenge, signature: `0x${string}`) {
  const credential = await requireCredential(deps, subjectKey);
  if (credential.method !== "vault") {
    throw new UnsupportedConfirmationMethodError();
  }
  const { accountId } = await relayRegisterAccount(deps.relayFlowDeps, challenge, signature);
  const account = await resyncUntil(deps, () => deps.projectionStore.getAccount(accountId));
  if (!account) {
    throw new Error(`account ${accountId} not found in projection immediately after registration`);
  }
  return account;
}

async function requireAccountIdentityOwner(deps: ChainLifecycleDeps, accountId: `0x${string}`, subjectKey: `0x${string}`): Promise<{ credential: Credential; account: ChainAccount }> {
  const credential = await requireCredential(deps, subjectKey);
  const account = await deps.projectionStore.getAccount(accountId);
  if (!account) {
    throw new LifecycleError("not_found", `account ${accountId} does not exist`);
  }
  const [identityId] = (await deps.relayFlowDeps.publicClient.readContract({
    address: deps.relayFlowDeps.identityRegistry,
    abi: identityRegistryAbi,
    functionName: "tracks",
    args: [account.trackId],
  })) as readonly [`0x${string}`, ...unknown[]];
  const identity = await deps.projectionStore.getIdentity(identityId);
  if (!identity) {
    throw new LifecycleError("not_found", `account ${accountId}'s track does not belong to a known identity`);
  }
  const predicted = await predictedAccountAddress(deps, credential);
  if (predicted.toLowerCase() !== identity.owner.toLowerCase()) {
    throw new LifecycleError("forbidden", `${subjectKey} does not own the identity behind account ${accountId}`);
  }
  return { credential, account };
}

/** Builds the EIP-712 challenge to activate `accountId` — only its
 * identity's real owner may request it. `eligibleFrom` is read live
 * from the account's own `registeredAt` (the contract requires
 * `eligibleFrom >= registeredAt`) rather than trusting a caller-chosen
 * value that could pre-date registration. */
export async function beginActivateAccount(deps: ChainLifecycleDeps, accountId: `0x${string}`, subjectKey: `0x${string}`, reasonHash: `0x${string}`): Promise<ActivateAccountChallenge> {
  await requireAccountIdentityOwner(deps, accountId, subjectKey);
  const [, , , , registeredAt] = (await deps.relayFlowDeps.publicClient.readContract({
    address: deps.relayFlowDeps.accountRegistry,
    abi: accountRegistryAbi,
    functionName: "accounts",
    args: [accountId],
  })) as readonly [`0x${string}`, `0x${string}`, `0x${string}`, number, bigint, ...unknown[]];
  return buildActivateAccountChallenge(deps.relayFlowDeps, accountId, registeredAt, reasonHash);
}

export async function activateAccount(deps: ChainLifecycleDeps, subjectKey: `0x${string}`, challenge: ActivateAccountChallenge, signature: `0x${string}`) {
  const credential = await requireCredential(deps, subjectKey);
  if (credential.method !== "vault") {
    throw new UnsupportedConfirmationMethodError();
  }
  await relayActivateAccount(deps.relayFlowDeps, challenge, signature);
  const account = await resyncUntil(deps, async () => {
    const found = await deps.projectionStore.getAccount(challenge.message.accountId);
    return found?.state === "active" ? found : null;
  });
  if (!account) {
    throw new Error(`account ${challenge.message.accountId} not found in projection immediately after activation`);
  }
  return account;
}

/** Builds the EIP-712 challenge to remove `accountId` — only its
 * identity's real owner may request it. */
export async function beginRemoveAccount(deps: ChainLifecycleDeps, accountId: `0x${string}`, subjectKey: `0x${string}`, reasonHash: `0x${string}`): Promise<RemoveAccountChallenge> {
  await requireAccountIdentityOwner(deps, accountId, subjectKey);
  return buildRemoveAccountChallenge(deps.relayFlowDeps, accountId, reasonHash);
}

export async function removeAccount(deps: ChainLifecycleDeps, subjectKey: `0x${string}`, challenge: RemoveAccountChallenge, signature: `0x${string}`) {
  const credential = await requireCredential(deps, subjectKey);
  if (credential.method !== "vault") {
    throw new UnsupportedConfirmationMethodError();
  }
  await relayRemoveAccount(deps.relayFlowDeps, challenge, signature);
  const account = await resyncUntil(deps, async () => {
    const found = await deps.projectionStore.getAccount(challenge.message.accountId);
    return found?.state === "removed" ? found : null;
  });
  if (!account) {
    throw new Error(`account ${challenge.message.accountId} not found in projection immediately after removal`);
  }
  return account;
}

// --- optional public name and bio (ProfileRegistry) ----------------------

export const MAX_PROFILE_NAME_BYTES = 40;
export const MAX_PROFILE_BIO_BYTES = 280;

function requireProfileRegistry(deps: ChainLifecycleDeps): `0x${string}` {
  if (!deps.profileRegistry) throw new LifecycleError("invalid_state", "profiles are not enabled on this deployment");
  return deps.profileRegistry;
}

/** The identity `subjectKey` owns on-chain, whatever its state — used
 * where the caller itself decides what to do with a still-pending
 * identity (e.g. resume onboarding at the right step instead of
 * offering to create a second one). */
export async function findMyIdentity(deps: ChainLifecycleDeps, subjectKey: `0x${string}`) {
  const credential = await deps.credentialStore.get(subjectKey);
  if (!credential) return null;
  const owner = await predictedAccountAddress(deps, credential);
  return deps.projectionStore.getIdentityByOwner(owner);
}

/** The identity `subjectKey` owns on-chain, if it has created one and
 * it is fully active (confirmed) — `null` for a pending one too. */
export async function findOwnedIdentity(deps: ChainLifecycleDeps, subjectKey: `0x${string}`) {
  const identity = await findMyIdentity(deps, subjectKey);
  return identity?.state === "active" ? identity : null;
}

/** What is written on-chain for the identity behind `subjectKey` — read live, never cached. */
export async function readIdentityProfile(deps: ChainLifecycleDeps, subjectKey: `0x${string}`): Promise<OnChainProfile | null> {
  if (!deps.profileRegistry) return null;
  const identity = await findOwnedIdentity(deps, subjectKey);
  if (!identity) return null;
  const profile = await readProfile(deps.relayFlowDeps.publicClient, deps.profileRegistry, identity.identityId);
  return profile.updatedAt === 0 ? null : profile;
}

export async function beginSetProfile(deps: ChainLifecycleDeps, subjectKey: `0x${string}`, name: string, bio: string): Promise<SetProfileChallenge> {
  const registry = requireProfileRegistry(deps);
  if (Buffer.byteLength(name) > MAX_PROFILE_NAME_BYTES) throw new LifecycleError("invalid_state", `the name can be at most ${MAX_PROFILE_NAME_BYTES} characters`);
  if (Buffer.byteLength(bio) > MAX_PROFILE_BIO_BYTES) throw new LifecycleError("invalid_state", `the bio can be at most ${MAX_PROFILE_BIO_BYTES} characters`);
  const identity = await findOwnedIdentity(deps, subjectKey);
  if (!identity) throw new LifecycleError("not_found", "create and activate your account first");
  return buildSetProfileChallenge(deps.relayFlowDeps, registry, identity.identityId, name, bio);
}

/** Relays the owner's signature over exactly the challenge `beginSetProfile` returned. */
export async function setIdentityProfile(deps: ChainLifecycleDeps, subjectKey: `0x${string}`, challenge: SetProfileChallenge, signature: `0x${string}`) {
  const registry = requireProfileRegistry(deps);
  const credential = await requireCredential(deps, subjectKey);
  if (credential.method !== "vault") throw new UnsupportedConfirmationMethodError();
  const identity = await findOwnedIdentity(deps, subjectKey);
  if (!identity || challenge.message.identityId !== identity.identityId || challenge.domain.verifyingContract.toLowerCase() !== registry.toLowerCase()) {
    throw new LifecycleError("forbidden", "this profile change is not for your identity");
  }
  await relaySetProfile(deps.relayFlowDeps, challenge, signature);
  // A load-balanced RPC can answer from a node one block behind the
  // transaction just mined — wait until what it returns is what was written.
  let profile = await readProfile(deps.relayFlowDeps.publicClient, registry, identity.identityId);
  for (let attempt = 0; attempt < 8 && (profile.name !== challenge.message.name || profile.bio !== challenge.message.bio); attempt++) {
    await new Promise((resolve) => setTimeout(resolve, 500));
    profile = await readProfile(deps.relayFlowDeps.publicClient, registry, identity.identityId);
  }
  return profile;
}
