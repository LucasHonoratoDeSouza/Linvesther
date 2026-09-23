// Thin fetch wrapper around apps/api, per the acceptance criteria: onboarding,
// failure, removal, disclosure and profile all work; USDT and the A0
// limitation are visible. Every function here returns a typed
// `ApiResult` rather than throwing, so a page can render a real failure
// state (the protocol: "isolar falha, expor estado e alerta sem inventar
// resultados") instead of crashing or fabricating a success.

import type {
  AuthenticationResponseJSON,
  PublicKeyCredentialCreationOptionsJSON,
  PublicKeyCredentialRequestOptionsJSON,
  RegistrationResponseJSON,
} from "./webauthn";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:4301";

export type ApiResult<T> = { ok: true; data: T } | { ok: false; status: number; error: string };

async function call<T>(path: string, init?: RequestInit): Promise<ApiResult<T>> {
  try {
    const response = await fetch(`${API_BASE_URL}${path}`, { ...init, credentials: "include" });
    const body = (await response.json().catch(() => ({}))) as Record<string, unknown>;
    if (!response.ok) {
      const code = typeof body.error === "string" ? body.error : `request failed with status ${response.status}`;
      // The broker's own rejection reason (e.g. what Coinbase/IBKR said,
      // never a secret) rides in `detail` — surfaced here so the UI
      // shows more than just a generic error code.
      const detail = typeof body.detail === "string" ? body.detail : typeof body.message === "string" ? body.message : null;
      return { ok: false, status: response.status, error: detail ? `${code}: ${detail}` : code };
    }
    return { ok: true, data: body as T };
  } catch (cause) {
    return { ok: false, status: 0, error: cause instanceof Error ? cause.message : "network error" };
  }
}

export function beginPasskeyRegistration(userName?: string): Promise<ApiResult<PublicKeyCredentialCreationOptionsJSON>> {
  return call("/auth/webauthn/register-options", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ userName }),
  });
}

export function finishPasskeyRegistration(response: RegistrationResponseJSON): Promise<ApiResult<{ csrfToken: string; address: `0x${string}` }>> {
  return call("/auth/webauthn/register-verify", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ response }),
  });
}

export function beginPasskeyLogin(): Promise<ApiResult<PublicKeyCredentialRequestOptionsJSON>> {
  return call("/auth/webauthn/login-options", { method: "POST" });
}

export function finishPasskeyLogin(response: AuthenticationResponseJSON): Promise<ApiResult<{ csrfToken: string; address: `0x${string}` }>> {
  return call("/auth/webauthn/login-verify", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ response }),
  });
}

/** Checks for an already-valid session (the cookie a prior sign-in
 * set), so a page can skip straight past its sign-in form instead of
 * re-prompting every time the user navigates to it. */
export function checkSession(): Promise<ApiResult<{ csrfToken: string; address: `0x${string}` }>> {
  return call("/auth/session");
}

/** Ends the current session, so the sign-in screen shows the picker
 * again instead of "you're signed in" for whichever identity a stale
 * session cookie belonged to. */
export function signOut(): Promise<ApiResult<{ ok: true }>> {
  return call("/auth/signout", { method: "POST" });
}

export function beginVaultChallenge(): Promise<ApiResult<{ challenge: string }>> {
  return call("/auth/vault/challenge", { method: "POST" });
}

export function verifyVaultSignature(qx: `0x${string}`, qy: `0x${string}`, challenge: string, signature: `0x${string}`): Promise<ApiResult<{ csrfToken: string; address: `0x${string}` }>> {
  return call("/auth/vault/verify", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ qx, qy, challenge, signature }),
  });
}

export interface Identity {
  identityId: `0x${string}`;
  owner: `0x${string}`;
  pendingOwner: `0x${string}` | null;
  ownerEpoch: string;
  state: "pending_confirmation" | "active";
}

export function createIdentity(): Promise<ApiResult<Identity>> {
  return call("/identities", { method: "POST" });
}

/** Whatever identity the signed-in session already owns on-chain, so a
 * page can resume at the right step instead of offering to create a
 * second one for someone who already has an identity. */
export function fetchMyIdentity(): Promise<ApiResult<{ identity: Identity | null }>> {
  return call("/identities/mine");
}

// ownerEpoch/nonce/deadline cross the wire as decimal strings (JSON has
// no bigint type) — signing the challenge needs the real bigint values
// back, see apps/web/app/onboarding/page.tsx's use of this type.
export interface ConfirmIdentityChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { ConfirmOwnerRotation: { name: string; type: string }[] };
  primaryType: "ConfirmOwnerRotation";
  message: { identityId: `0x${string}`; newOwner: `0x${string}`; ownerEpoch: string; nonce: string; deadline: string };
}

export function beginConfirmIdentity(identityId: string): Promise<ApiResult<ConfirmIdentityChallenge>> {
  return call(`/identities/${identityId}/confirm-challenge`, { method: "POST" });
}

export function confirmIdentity(identityId: string, challenge: ConfirmIdentityChallenge, signature: `0x${string}`): Promise<ApiResult<Identity>> {
  return call(`/identities/${identityId}/confirm`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ challenge, signature }),
  });
}

export interface TrackProjection {
  profile: { identityId: string; trackId: string; createdAt: string; verifiedSince: string | null; currency: string };
  dimensions: { origin: string; coverage: string; calculation: string; registry: string; availability: string };
  gaps: { startMs: number; endMs: number }[];
  corrections: { targetDigest: string; reasonCode: string; effectiveFrom: string }[];
}

export function fetchPublicTrack(trackId: string): Promise<ApiResult<TrackProjection>> {
  return call(`/public/tracks/${trackId}`);
}

export interface ClaimSet {
  identityId: string;
  trackId: string;
  checkpointId: string;
  periodStart: string;
  periodEnd: string;
  claims: { type: string; metric: string; threshold?: string; value?: string; min?: string; max?: string }[];
  audience: string;
  nonce: string;
  expiresAt: string;
}

// Mirrors apps/api/src/auth/identitySignature.ts's IdentityCredentialProof —
// the same credential proof used for login also authorizes disclosure
// (apps/api/src/claims/disclosure.ts's verifyConsent), so /claims/disclose
// takes a credential here, not a bare signature.
export type IdentityCredentialProof =
  | { method: "vault"; signature: `0x${string}` }
  | { method: "webauthn"; response: AuthenticationResponseJSON };

export function discloseClaimSet(claimSet: ClaimSet, credential: IdentityCredentialProof): Promise<ApiResult<{ digest: string }>> {
  return call("/claims/disclose", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ claimSet, credential }),
  });
}

export interface BinanceConnectResult {
  connectionId: string;
  summary: { tradesFetched: number; flowsFetched: number; catalogSymbolsFetched: number };
}

export function connectBinance(accountId: string, apiKey: string, apiSecret: string, label?: string): Promise<ApiResult<BinanceConnectResult>> {
  return call(`/accounts/${accountId}/binance-connection`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ apiKey, apiSecret, label }),
  });
}

export function connectCoinbase(accountId: string, keyName: string, privateKey: string, label?: string): Promise<ApiResult<BinanceConnectResult>> {
  return call(`/accounts/${accountId}/coinbase-connection`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ keyName, privateKey, label }),
  });
}

export function connectKraken(accountId: string, apiKey: string, apiSecret: string, label?: string): Promise<ApiResult<BinanceConnectResult>> {
  return call(`/accounts/${accountId}/kraken-connection`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ apiKey, apiSecret, label }),
  });
}

export function renameAccount(accountId: string, label: string): Promise<ApiResult<{ label: string }>> {
  return call(`/accounts/${accountId}/label`, {
    method: "PATCH",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ label }),
  });
}

/** Removes a connected account: its stored credential and everything collected for it. */
export function removeAccount(accountId: string): Promise<ApiResult<{ removed: boolean; broker: string }>> {
  return call(`/accounts/${accountId}`, { method: "DELETE" });
}

export function connectIbkr(accountId: string, token: string, queryId: string, label?: string): Promise<ApiResult<BinanceConnectResult>> {
  return call(`/accounts/${accountId}/ibkr-connection`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ token, queryId, label }),
  });
}

/** One exchange account connected under the signed-in identity. */
export interface ConnectedAccount {
  accountId: string;
  /** Which broker/exchange this account is at (see app/portfolio/brokers.ts). */
  broker: string;
  label: string | null;
  /** Epoch ms of the connection — every figure for this account is measured from then. */
  connectedAtMs: number;
  status: string;
}

export function fetchAccounts(): Promise<ApiResult<{ accounts: ConnectedAccount[] }>> {
  return call("/accounts");
}

export interface BinanceConnectionStatus {
  connected: boolean;
  status: string | null;
  last_synced_at: string | null;
  trade_count: number;
  flow_count: number;
}

export function fetchBinanceConnection(accountId: string): Promise<ApiResult<BinanceConnectionStatus>> {
  return call(`/accounts/${accountId}/binance-connection`);
}

/** Pulls the latest deposits/withdrawals from the exchange now (the periodic sync would only catch up later). */
export function syncBinanceAccount(accountId: string): Promise<ApiResult<{ flowsFetched: number }>> {
  return call(`/accounts/${accountId}/binance-sync`, { method: "POST" });
}

export interface BinanceNav {
  nav: string;
  currency: string;
  assets: { asset: string; quantity: string; price: string; value: string }[];
  excludedOutOfScopeAssets: string[];
}

export function fetchBinanceNav(accountId: string): Promise<ApiResult<BinanceNav>> {
  return call(`/accounts/${accountId}/binance-nav`);
}

/** Every metric here is independently `null`, paired with a
 * `*Unavailable` reason, when its own minimum sample isn't met yet —
 * a real refusal, not a fabricated number standing in for one. */
export interface BinancePerformance {
  dailyNav: { dayStartMs: number; nav: string }[];
  dailyReturns: string[];
  sharpe: string | null;
  sharpeUnavailable: string | null;
  sortino: string | null;
  sortinoUnavailable: string | null;
  maxDrawdown: string | null;
  cagr: string | null;
  cagrUnavailable: string | null;
  winRate: { closedRoundTrips: number; wins: number; winRate: string } | null;
  /** The instant the account was connected — every figure is measured from then on. */
  earliestReliableCheckpointMs: number | null;
  /** Profit since the connection, in USDT (average-cost basis; what was held at connection starts flat). */
  pnl: {
    positions: {
      asset: string;
      quantity: string;
      averageCost: string | null;
      markPrice: string;
      realized: string;
      unrealized: string | null;
    }[];
    realized: string;
    unrealized: string;
    fees: string;
  };
}

export function fetchBinancePerformance(accountId: string): Promise<ApiResult<BinancePerformance>> {
  return call(`/accounts/${accountId}/binance-performance`);
}

/** A portable, independently-verifiable ZK proof of return/max-drawdown
 * over the account's real history — never the raw trades, balance or
 * account identifier (see apps/api/src/binance-connect's README).
 * `receiptHex`/`journalHex` are the same bincode+hex format
 * `crates/verifier` expects, so a third party can verify this without
 * trusting this app again. NOTE: the underlying real RISC Zero proving
 * this calls can take a very long time (hours, not seconds, observed
 * during this delivery) — there is intentionally no button wired to
 * this on `/portfolio` yet, since a synchronous "click and wait" UI
 * would misrepresent that wait. A real UI for this needs an async job
 * + polling pattern, not a direct request/response call. */
export interface BinancePerformanceProof {
  envelopeDigestHex: string;
  periodStartMs: number;
  periodEndMs: number;
  returnFraction: string;
  maxDrawdownBp: number;
  imageIdHex: string;
  receiptHex: string;
  journalHex: string;
}

export function fetchBinancePerformanceProof(accountId: string): Promise<ApiResult<BinancePerformanceProof>> {
  return call(`/accounts/${accountId}/binance-performance-proof`);
}

export type ShareableMetric = "return" | "maxDrawdown" | "sharpe" | "winRate";

/** The owner's one privacy choice: in full privacy mode the public address shows nothing. */
export interface ProfileSettings {
  privacyMode: boolean;
}

/** What anyone can read about an account — mirrors
 * apps/api/src/profile/types.ts. Only ratios, never an absolute amount. */
export interface ProfileStatement {
  address: `0x${string}`;
  accountId: string;
  trackName: string;
  since: string;
  trackedDays: number;
  verification: "collector_attested_read_only";
  metrics: {
    return?: string;
    maxDrawdown?: string;
    sharpe?: string;
    winRate?: { value: string; closedTrades: number };
  };
  computedAt: string;
}

export interface PublicTrack {
  statement: ProfileStatement;
  /** Metrics switched on but not computable yet (not enough history). */
  unavailable: ShareableMetric[];
}

export function fetchProfileSettings(): Promise<ApiResult<{ address: `0x${string}` } & ProfileSettings>> {
  return call("/profile/settings");
}

export function updateProfileSettings(settings: ProfileSettings): Promise<ApiResult<{ address: `0x${string}` } & ProfileSettings>> {
  return call("/profile/settings", {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(settings),
  });
}

/** The optional public name and bio an owner wrote on-chain. */
export interface OnChainProfile {
  name: string;
  bio: string;
  updatedAt: number;
}

export interface SetProfileChallenge {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { SetProfile: { name: string; type: string }[] };
  primaryType: "SetProfile";
  message: { identityId: `0x${string}`; name: string; bio: string; nonce: string; deadline: string };
}

export function fetchIdentityProfile(): Promise<ApiResult<{ enabled: boolean; active: boolean; profile: OnChainProfile | null }>> {
  return call("/profile/identity");
}

export function beginSetProfile(name: string, bio: string): Promise<ApiResult<SetProfileChallenge>> {
  return call("/profile/identity/challenge", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name, bio }),
  });
}

export function submitProfile(challenge: SetProfileChallenge, signature: `0x${string}`): Promise<ApiResult<{ profile: OnChainProfile }>> {
  return call("/profile/identity", {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ challenge, signature }),
  });
}

/** Which collector signed the figures behind a profile or a claim. */
export interface CollectorOrigin {
  mechanism: "A0";
  collector: string | null;
}

export function fetchPublicProfile(
  address: string,
): Promise<ApiResult<{ address: `0x${string}`; privacyMode: boolean; profile: OnChainProfile | null; tracks: PublicTrack[]; origin?: CollectorOrigin }>> {
  return call(`/public/profiles/${address}`);
}

/** The combined return curve of every public account — percentages only. */
export function fetchPublicSeries(
  address: string,
  range: SeriesRange,
): Promise<ApiResult<{ points: { timeMs: number; returnFraction: string }[]; stepMs: number; sinceMs: number }>> {
  return call(`/public/profiles/${address}/series?range=${range}`);
}

export type SeriesRange = "24h" | "7d" | "30d" | "1y" | "5y" | "max";

/** The account's value and return over a range, drawn from real market candles. */
export interface BinanceSeries {
  points: { timeMs: number; nav: string; index: string }[];
  stepMs: number;
  /** When the account was connected — no range reaches back before it. */
  sinceMs: number;
}

export function fetchBinanceSeries(accountId: string, range: SeriesRange): Promise<ApiResult<BinanceSeries>> {
  return call(`/accounts/${accountId}/binance-series?range=${range}`);
}

export interface ClaimContext {
  address: `0x${string}`;
  current: { metrics: Record<string, number | undefined>; since: string; computedAt: string } | null;
}

export function fetchClaimContext(): Promise<ApiResult<ClaimContext>> {
  return call("/claims/context");
}

export interface MyClaim {
  digest: string;
  claimSet: ClaimSet;
  publishedAt: string | null;
}

export function fetchMyClaims(): Promise<ApiResult<{ claims: MyClaim[] }>> {
  return call("/claims/mine");
}

export interface PublicClaim {
  digest: string;
  owner: `0x${string}`;
  claimSet: ClaimSet;
  publishedAt: string | null;
  verification: string;
  origin: CollectorOrigin;
}

export function fetchPublicClaim(digest: string): Promise<ApiResult<PublicClaim>> {
  return call(`/public/claims/${digest}`);
}
