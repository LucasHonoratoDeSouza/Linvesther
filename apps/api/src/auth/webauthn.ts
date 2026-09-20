import { MemoryWebAuthnCredentialStore, type WebAuthnCredentialStore } from "./credentialStore.js";
import {
  generateAuthenticationOptions,
  generateRegistrationOptions,
  verifyAuthenticationResponse,
  verifyRegistrationResponse,
  type AuthenticationResponseJSON,
  type PublicKeyCredentialCreationOptionsJSON,
  type PublicKeyCredentialRequestOptionsJSON,
  type RegistrationResponseJSON,
} from "@simplewebauthn/server";
import { COSEALG, cose, decodeCredentialPublicKey, isoBase64URL } from "@simplewebauthn/server/helpers";
import { MemoryNonceStore, type NonceStore } from "./nonceStore.js";

export interface WebAuthnConfig {
  rpName: string;
  rpID: string;
  origin: string;
}

// A real browser always sends the
// exact scheme+host+port it's served from as clientDataJSON.origin, so
// a real e2e run (Next dev, no TLS, a port) can never match a hardcoded
// "https://localhost" — the mitigation prescribed there is the same
// env-driven override pattern already used by SIWE_DOMAIN/
// NEXT_PUBLIC_SIWE_DOMAIN, not a new infrastructure decision. Falls
// back to the previous hardcoded values so nothing changes for a
// caller that doesn't set these (e.g. apps/api/test/webauthn.test.ts,
// tests/e2e/auth/auth.e2e.test.ts, both of which build their own
// clientDataJSON against "https://localhost" directly).
const DEFAULT_CONFIG: WebAuthnConfig = {
  rpName: "LinvestherZK",
  rpID: process.env.WEBAUTHN_RP_ID ?? "localhost",
  origin: process.env.WEBAUTHN_ORIGIN ?? "https://localhost",
};

// Registration and authentication challenges are tracked separately
// (two ceremonies, two single-use nonce pools), same "issue once, consume
// once" pattern as nonceStore.ts, reused as its own instances here since
// the library needs a predicate function rather than an explicit value.
let registrationChallenges: NonceStore = new MemoryNonceStore();
let authenticationChallenges: NonceStore = new MemoryNonceStore();

/** Chooses where passkey challenges are kept. In memory by default; a
 * deployment with a database passes shared stores. */
export function useWebAuthnNonceStores(stores: { registration: NonceStore; authentication: NonceStore }): void {
  registrationChallenges = stores.registration;
  authenticationChallenges = stores.authentication;
}

// Keyed by WebAuthn credential ID (not subjectKey) — this is bookkeeping
// internal to the ceremony (the library needs a credential's COSE public
// key and counter to verify a later assertion), separate from
// credentialStore.ts's subjectKey-keyed cross-route store.
let credentialsById: WebAuthnCredentialStore = new MemoryWebAuthnCredentialStore();

/** Chooses where registered passkeys are kept. In memory by default; a
 * deployment with a database passes a durable store so passkeys survive
 * a restart. */
export function useWebAuthnCredentialStore(store: WebAuthnCredentialStore): void {
  credentialsById = store;
}

function toHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
}

/** Extracts (qx, qy) from a COSE-encoded EC2/P-256 public key, or
 * `undefined` if the key is not a valid EC2 key with both coordinates. */
function ec2Coordinates(publicKeyCose: Uint8Array<ArrayBuffer>): { qx: `0x${string}`; qy: `0x${string}` } | undefined {
  const coseKey = decodeCredentialPublicKey(publicKeyCose);
  if (!cose.isCOSEPublicKeyEC2(coseKey)) {
    return undefined;
  }
  const x = coseKey.get(cose.COSEKEYS.x);
  const y = coseKey.get(cose.COSEKEYS.y);
  if (!x || !y) {
    return undefined;
  }
  return { qx: toHex(x), qy: toHex(y) };
}

/** Prepares a registration ceremony: any authenticator the browser offers
 * (biometrics, PIN, security key) is accepted — no `authenticatorSelection`
 * constraint narrows this to a specific authenticator type. `supportedAlgorithmIDs`
 * is pinned to ES256 (P-256) only — the design fixes P-256
 * as the single curve for both credential methods (what SignerP256/
 * SignerWebAuthn's on-chain verification expects), but the library's own
 * default offers EdDSA first; an authenticator that supports both would
 * otherwise be free to register a non-P-256 key this backend can't verify. */
export async function beginRegistration(
  userName: string,
  config: WebAuthnConfig = DEFAULT_CONFIG,
): Promise<PublicKeyCredentialCreationOptionsJSON> {
  const challenge = await registrationChallenges.issue();
  return generateRegistrationOptions({
    rpName: config.rpName,
    rpID: config.rpID,
    userName,
    challenge,
    attestationType: "none",
    supportedAlgorithmIDs: [COSEALG.ES256],
  });
}

export interface WebAuthnRegistrationResult {
  verified: boolean;
  qx?: `0x${string}`;
  qy?: `0x${string}`;
  credentialId?: string;
}

/** Verifies a registration response via @simplewebauthn/server, never
 * touching a private key (WebAuthn never sends one to the server). On
 * success, the credential's COSE public key is cached under its
 * credential ID for later `finishAuthentication` calls, and (qx, qy) are
 * returned for the caller to persist wherever it keys credentials by
 * subjectKey. Any verification failure (bad signature, wrong/replayed
 * challenge, wrong origin/RP ID) returns `{ verified: false }` rather
 * than throwing, leaving no partial state behind. */
export async function finishRegistration(
  response: RegistrationResponseJSON,
  config: WebAuthnConfig = DEFAULT_CONFIG,
): Promise<WebAuthnRegistrationResult> {
  let verification;
  try {
    verification = await verifyRegistrationResponse({
      response,
      expectedChallenge: (challenge) => registrationChallenges.consume(isoBase64URL.toUTF8String(challenge)),
      expectedOrigin: config.origin,
      expectedRPID: config.rpID,
    });
  } catch {
    return { verified: false };
  }
  if (!verification.verified || !verification.registrationInfo) {
    return { verified: false };
  }

  const { credential } = verification.registrationInfo;
  const coordinates = ec2Coordinates(credential.publicKey);
  if (!coordinates) {
    return { verified: false };
  }

  await credentialsById.save(credential.id, { publicKeyCose: credential.publicKey, counter: credential.counter });

  return { verified: true, qx: coordinates.qx, qy: coordinates.qy, credentialId: credential.id };
}

export async function beginAuthentication(
  config: WebAuthnConfig = DEFAULT_CONFIG,
): Promise<PublicKeyCredentialRequestOptionsJSON> {
  const challenge = await authenticationChallenges.issue();
  return generateAuthenticationOptions({ rpID: config.rpID, challenge });
}

export interface WebAuthnAuthenticationResult {
  verified: boolean;
  qx?: `0x${string}`;
  qy?: `0x${string}`;
}

/** Verifies an authentication (login) assertion against the credential
 * registered under `response.id` — an unknown credential ID, a
 * wrong/replayed challenge, or an invalid signature all return
 * `{ verified: false }`. */
export async function finishAuthentication(
  response: AuthenticationResponseJSON,
  config: WebAuthnConfig = DEFAULT_CONFIG,
): Promise<WebAuthnAuthenticationResult> {
  const stored = await credentialsById.get(response.id);
  if (!stored) {
    return { verified: false };
  }

  let verification;
  try {
    verification = await verifyAuthenticationResponse({
      response,
      expectedChallenge: (challenge) => authenticationChallenges.consume(isoBase64URL.toUTF8String(challenge)),
      expectedOrigin: config.origin,
      expectedRPID: config.rpID,
      credential: { id: response.id, publicKey: stored.publicKeyCose, counter: stored.counter },
    });
  } catch {
    return { verified: false };
  }
  if (!verification.verified) {
    return { verified: false };
  }

  await credentialsById.updateCounter(response.id, verification.authenticationInfo.newCounter);

  const coordinates = ec2Coordinates(stored.publicKeyCose);
  if (!coordinates) {
    return { verified: false };
  }
  return { verified: true, qx: coordinates.qx, qy: coordinates.qy };
}
