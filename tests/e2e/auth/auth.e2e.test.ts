import { buildApp, RateLimiter } from "@linvestherzk/api";
import type {
  AuthenticationResponseJSON,
  RegistrationResponseJSON,
} from "@simplewebauthn/server";
import { isoBase64URL, isoCBOR } from "@simplewebauthn/server/helpers";
import { concat, getAddress, keccak256, slice } from "viem";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

// End-to-end, per T8's "Done when": vault register+login via .inject()
// (no browser — pure WebCrypto in the test) covering challenge replay
// and cross-identity, in the same pattern as the SIWE test this
// replaces. Plus whatever WebAuthn route coverage is reasonably
// testable without a real browser (the ceremony internals themselves
// are already covered by apps/api/test/webauthn.test.ts — this file's
// job is wiring + session issuance).
// Each test drives the real Fastify app through `.inject()` (a full,
// real HTTP request/response cycle through routing, cookie parsing and
// the body-size limit — not a call directly into a handler function).

const DOMAIN = "app.linvestherzk.example";
const NOW = new Date("2026-01-01T00:05:00Z");

interface VaultKeyPair {
  privateKey: CryptoKey;
  qx: `0x${string}`;
  qy: `0x${string}`;
}

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
}

async function generateVaultKeyPair(): Promise<VaultKeyPair> {
  const { publicKey, privateKey } = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  return {
    privateKey,
    qx: bytesToHex(raw.slice(1, 33)),
    qy: bytesToHex(raw.slice(33, 65)),
  };
}

async function signVaultChallenge(
  privateKey: CryptoKey,
  challenge: string,
): Promise<`0x${string}`> {
  const signature = await crypto.subtle.sign(
    { name: "ECDSA", hash: "SHA-256" },
    privateKey,
    new TextEncoder().encode(challenge),
  );
  return bytesToHex(new Uint8Array(signature));
}

// Independent re-derivation of the design decision:
// subjectKey = last20Bytes(keccak256(qx || qy)), checksummed. Computed
// here from the spec formula, not imported from the implementation, so
// this test verifies the API's actual session identity against the
// spec rather than mirroring whatever app.ts happens to do.
function expectedSubjectKey(
  qx: `0x${string}`,
  qy: `0x${string}`,
): `0x${string}` {
  return getAddress(slice(keccak256(concat([qx, qy])), 12));
}

// --- real P-256 key + WebAuthn wire-format helpers, mirroring
// apps/api/test/webauthn.test.ts's approach (real CBOR attestation
// object / COSE public key, DER-wrapped ECDSA signature) against the
// same rpID/origin app.ts's DEFAULT_CONFIG uses ("localhost" /
// "https://localhost"). Only enough to prove the route wiring — not a
// re-test of ceremony internals, which webauthn.test.ts already covers.

const RP_ID = "localhost";
const ORIGIN = "https://localhost";

async function generateWebAuthnKeyPair() {
  const { publicKey, privateKey } = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  return { privateKey, qx: raw.slice(1, 33), qy: raw.slice(33, 65) };
}

function concatBytes(
  ...parts: Uint8Array<ArrayBuffer>[]
): Uint8Array<ArrayBuffer> {
  const total = parts.reduce((sum, p) => sum + p.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function encodeCosePublicKey(
  qx: Uint8Array<ArrayBuffer>,
  qy: Uint8Array<ArrayBuffer>,
): Uint8Array<ArrayBuffer> {
  const coseKey = new Map<number, number | Uint8Array<ArrayBuffer>>([
    [1, 2], // kty: EC2
    [3, -7], // alg: ES256
    [-1, 1], // crv: P-256
    [-2, qx],
    [-3, qy],
  ]);
  return isoCBOR.encode(coseKey) as Uint8Array<ArrayBuffer>;
}

async function buildAuthenticatorData(opts: {
  flags: number;
  counter: number;
  credentialId?: Uint8Array<ArrayBuffer>;
  cosePublicKey?: Uint8Array<ArrayBuffer>;
}): Promise<Uint8Array<ArrayBuffer>> {
  const rpIdHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(RP_ID)),
  );
  const flagsByte = new Uint8Array([opts.flags]);
  const counterBytes = new Uint8Array(4);
  new DataView(counterBytes.buffer).setUint32(0, opts.counter, false);

  if (opts.credentialId && opts.cosePublicKey) {
    const aaguid = new Uint8Array(16);
    const credIdLen = new Uint8Array(2);
    new DataView(credIdLen.buffer).setUint16(
      0,
      opts.credentialId.length,
      false,
    );
    return concatBytes(
      rpIdHash,
      flagsByte,
      counterBytes,
      aaguid,
      credIdLen,
      opts.credentialId,
      opts.cosePublicKey,
    );
  }
  return concatBytes(rpIdHash, flagsByte, counterBytes);
}

function buildClientDataJSON(
  type: "webauthn.create" | "webauthn.get",
  challenge: string,
): Uint8Array<ArrayBuffer> {
  return new TextEncoder().encode(
    JSON.stringify({ type, challenge, origin: ORIGIN }),
  );
}

function derEncodeInteger(
  bytes: Uint8Array<ArrayBuffer>,
): Uint8Array<ArrayBuffer> {
  let start = 0;
  while (
    start < bytes.length - 1 &&
    bytes[start] === 0 &&
    (bytes[start + 1] ?? 0) < 0x80
  ) {
    start++;
  }
  let trimmed = bytes.slice(start);
  if ((trimmed[0] ?? 0) & 0x80) {
    trimmed = concatBytes(new Uint8Array([0]), trimmed);
  }
  return concatBytes(new Uint8Array([0x02, trimmed.length]), trimmed);
}

function derEncodeSignature(
  rawSignature: Uint8Array<ArrayBuffer>,
): Uint8Array<ArrayBuffer> {
  const r = rawSignature.slice(0, 32);
  const s = rawSignature.slice(32, 64);
  return concatBytes(
    new Uint8Array([
      0x30,
      derEncodeInteger(r).length + derEncodeInteger(s).length,
    ]),
    derEncodeInteger(r),
    derEncodeInteger(s),
  );
}

async function buildRegistrationResponse(
  keyPair: { qx: Uint8Array<ArrayBuffer>; qy: Uint8Array<ArrayBuffer> },
  challenge: string,
  credentialId: Uint8Array<ArrayBuffer>,
): Promise<RegistrationResponseJSON> {
  const cosePublicKey = encodeCosePublicKey(keyPair.qx, keyPair.qy);
  const authenticatorData = await buildAuthenticatorData({
    flags: 0x45,
    counter: 0,
    credentialId,
    cosePublicKey,
  });
  const clientDataJSON = buildClientDataJSON("webauthn.create", challenge);
  const attestationObject = isoCBOR.encode(
    new Map<string, string | Map<never, never> | Uint8Array<ArrayBuffer>>([
      ["fmt", "none"],
      ["attStmt", new Map<never, never>()],
      ["authData", authenticatorData],
    ]),
  );

  return {
    id: isoBase64URL.fromBuffer(credentialId),
    rawId: isoBase64URL.fromBuffer(credentialId),
    response: {
      clientDataJSON: isoBase64URL.fromBuffer(clientDataJSON),
      attestationObject: isoBase64URL.fromBuffer(attestationObject),
    },
    clientExtensionResults: {},
    type: "public-key",
  };
}

async function buildAuthenticationResponse(
  privateKey: CryptoKey,
  challenge: string,
  credentialId: Uint8Array<ArrayBuffer>,
): Promise<AuthenticationResponseJSON> {
  const authenticatorData = await buildAuthenticatorData({
    flags: 0x05,
    counter: 0,
  });
  const clientDataJSON = buildClientDataJSON("webauthn.get", challenge);
  const clientDataHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", clientDataJSON),
  );
  const signedData = concatBytes(authenticatorData, clientDataHash);
  const rawSignature = new Uint8Array(
    await crypto.subtle.sign(
      { name: "ECDSA", hash: "SHA-256" },
      privateKey,
      signedData,
    ),
  );
  const signature = derEncodeSignature(rawSignature);

  return {
    id: isoBase64URL.fromBuffer(credentialId),
    rawId: isoBase64URL.fromBuffer(credentialId),
    response: {
      clientDataJSON: isoBase64URL.fromBuffer(clientDataJSON),
      authenticatorData: isoBase64URL.fromBuffer(authenticatorData),
      signature: isoBase64URL.fromBuffer(signature),
    },
    clientExtensionResults: {},
    type: "public-key",
  };
}

async function buildTestApp(
  overrides: Partial<Parameters<typeof buildApp>[0]> = {},
) {
  const mineKeyPair = await generateVaultKeyPair();
  const theirsKeyPair = await generateVaultKeyPair();
  const app = buildApp({
    domain: DOMAIN,
    now: () => NOW,
    accountOwners: new Map([
      ["acc-mine", expectedSubjectKey(mineKeyPair.qx, mineKeyPair.qy)],
      ["acc-theirs", expectedSubjectKey(theirsKeyPair.qx, theirsKeyPair.qy)],
    ]),
    ...overrides,
  });
  await app.ready();
  return { app, mineKeyPair, theirsKeyPair };
}

async function vaultChallenge(app: Awaited<ReturnType<typeof buildApp>>) {
  const response = await app.inject({
    method: "POST",
    url: "/auth/vault/challenge",
  });
  const { challenge } = response.json() as { challenge: string };
  return challenge;
}

async function vaultSignIn(
  app: Awaited<ReturnType<typeof buildApp>>,
  keyPair: VaultKeyPair,
) {
  const challenge = await vaultChallenge(app);
  const signature = await signVaultChallenge(keyPair.privateKey, challenge);
  const verifyResponse = await app.inject({
    method: "POST",
    url: "/auth/vault/verify",
    payload: { qx: keyPair.qx, qy: keyPair.qy, challenge, signature },
  });
  const setCookie = verifyResponse.headers["set-cookie"];
  const cookie = Array.isArray(setCookie) ? setCookie[0] : setCookie;
  const sid = cookie?.split(";")[0];
  const { csrfToken } = verifyResponse.json() as { csrfToken: string };
  return { verifyResponse, challenge, signature, cookie: sid!, csrfToken };
}

describe("Vault sign-in", () => {
  let app: Awaited<ReturnType<typeof buildTestApp>>["app"];
  let mineKeyPair: VaultKeyPair;

  beforeEach(async () => {
    ({ app, mineKeyPair } = await buildTestApp());
  });
  afterEach(async () => {
    await app.close();
  });

  it("a valid vault signature issues a session under the expected subjectKey", async () => {
    const { verifyResponse, cookie } = await vaultSignIn(app, mineKeyPair);
    expect(verifyResponse.statusCode).toBe(200);
    expect(cookie).toBeTruthy();

    const session = await app.inject({
      method: "GET",
      url: "/auth/session",
      cookies: { sid: cookie.split("=")[1]! },
    });
    expect(session.statusCode).toBe(200);
    expect(session.json().address).toBe(
      expectedSubjectKey(mineKeyPair.qx, mineKeyPair.qy),
    );
  });

  it("a replayed vault challenge is rejected on the second attempt", async () => {
    const challenge = await vaultChallenge(app);
    const signature = await signVaultChallenge(
      mineKeyPair.privateKey,
      challenge,
    );

    const first = await app.inject({
      method: "POST",
      url: "/auth/vault/verify",
      payload: { qx: mineKeyPair.qx, qy: mineKeyPair.qy, challenge, signature },
    });
    expect(first.statusCode).toBe(200);

    const replay = await app.inject({
      method: "POST",
      url: "/auth/vault/verify",
      payload: { qx: mineKeyPair.qx, qy: mineKeyPair.qy, challenge, signature },
    });
    expect(replay.statusCode).toBe(401);
    expect(replay.json()).toEqual({ error: "signature_invalid" });
  });

  it("a signature from a different identity's key than the presented (qx, qy) is rejected", async () => {
    const stranger = await generateVaultKeyPair();
    const challenge = await vaultChallenge(app);
    const signature = await signVaultChallenge(stranger.privateKey, challenge);

    const response = await app.inject({
      method: "POST",
      url: "/auth/vault/verify",
      payload: { qx: mineKeyPair.qx, qy: mineKeyPair.qy, challenge, signature },
    });
    expect(response.statusCode).toBe(401);
    expect(response.json()).toEqual({ error: "signature_invalid" });
  });

  it("a returning identity (already registered) can sign in again with a fresh challenge", async () => {
    await vaultSignIn(app, mineKeyPair);
    const second = await vaultSignIn(app, mineKeyPair);
    expect(second.verifyResponse.statusCode).toBe(200);
  });
});

describe("WebAuthn sign-in wiring", () => {
  let app: Awaited<ReturnType<typeof buildTestApp>>["app"];

  beforeEach(async () => {
    ({ app } = await buildTestApp());
  });
  afterEach(async () => {
    await app.close();
  });

  it("register-options returns a fresh challenge for the given userName", async () => {
    const response = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-options",
      payload: { userName: "alice" },
    });
    expect(response.statusCode).toBe(200);
    const body = response.json() as {
      challenge: string;
      user: { name: string };
    };
    expect(body.challenge).toBeTruthy();
    expect(body.user.name).toBe("alice");
  });

  it("login-options returns a fresh challenge", async () => {
    const response = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-options",
    });
    expect(response.statusCode).toBe(200);
    const body = response.json() as { challenge: string };
    expect(body.challenge).toBeTruthy();
  });

  it("register-verify with a valid attestation issues a session and registers the credential", async () => {
    const keyPair = await generateWebAuthnKeyPair();
    const credentialId = new Uint8Array([1, 2, 3, 4]);
    const options = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-options",
      payload: { userName: "alice" },
    });
    const { challenge } = options.json() as { challenge: string };
    const response = await buildRegistrationResponse(
      keyPair,
      challenge,
      credentialId,
    );

    const verify = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-verify",
      payload: { response },
    });
    expect(verify.statusCode).toBe(200);
    const { csrfToken } = verify.json() as { csrfToken: string };
    expect(csrfToken).toBeTruthy();
    const setCookie = verify.headers["set-cookie"];
    expect(setCookie).toBeTruthy();
  });

  it("login-verify with a valid assertion from a previously registered credential issues a session", async () => {
    const keyPair = await generateWebAuthnKeyPair();
    const credentialId = new Uint8Array([9, 9, 9, 9]);
    const regOptions = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-options",
      payload: { userName: "alice" },
    });
    const regResponse = await buildRegistrationResponse(
      keyPair,
      (regOptions.json() as { challenge: string }).challenge,
      credentialId,
    );
    await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-verify",
      payload: { response: regResponse },
    });

    const loginOptions = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-options",
    });
    const authResponse = await buildAuthenticationResponse(
      keyPair.privateKey,
      (loginOptions.json() as { challenge: string }).challenge,
      credentialId,
    );

    const verify = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-verify",
      payload: { response: authResponse },
    });
    expect(verify.statusCode).toBe(200);
    const { csrfToken } = verify.json() as { csrfToken: string };
    expect(csrfToken).toBeTruthy();
  });

  it("register-verify with an invalid response is rejected without creating a session", async () => {
    const response = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-verify",
      payload: {
        response: {
          id: "bogus",
          rawId: "bogus",
          response: {},
          clientExtensionResults: {},
          type: "public-key",
        },
      },
    });
    expect(response.statusCode).toBe(401);
    expect(response.json()).toEqual({ error: "signature_invalid" });
    expect(response.headers["set-cookie"]).toBeUndefined();
  });

  it("login-verify with an invalid response is rejected without creating a session", async () => {
    const response = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-verify",
      payload: {
        response: {
          id: "bogus",
          rawId: "bogus",
          response: {},
          clientExtensionResults: {},
          type: "public-key",
        },
      },
    });
    expect(response.statusCode).toBe(401);
    expect(response.json()).toEqual({ error: "signature_invalid" });
    expect(response.headers["set-cookie"]).toBeUndefined();
  });

  // Closes the remaining gap not covered by
  // apps/api/test/webauthn.test.ts's ceremony-internals unit tests —
  // this exercises the same "issue once, consume once" challenge
  // replay guarantee SIWE's nonce had, but through the real HTTP route
  // wiring rather than calling finishAuthentication() directly.
  it("a replayed login-verify assertion is rejected on the second attempt", async () => {
    const keyPair = await generateWebAuthnKeyPair();
    const credentialId = new Uint8Array([7, 7, 7, 7]);
    const regOptions = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-options",
      payload: { userName: "alice" },
    });
    const regResponse = await buildRegistrationResponse(
      keyPair,
      (regOptions.json() as { challenge: string }).challenge,
      credentialId,
    );
    await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-verify",
      payload: { response: regResponse },
    });

    const loginOptions = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-options",
    });
    const authResponse = await buildAuthenticationResponse(
      keyPair.privateKey,
      (loginOptions.json() as { challenge: string }).challenge,
      credentialId,
    );

    const first = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-verify",
      payload: { response: authResponse },
    });
    expect(first.statusCode).toBe(200);

    const replay = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-verify",
      payload: { response: authResponse },
    });
    expect(replay.statusCode).toBe(401);
    expect(replay.json()).toEqual({ error: "signature_invalid" });
    expect(replay.headers["set-cookie"]).toBeUndefined();
  });

  // Cross-identity case for WebAuthn (the vault describe block
  // above already covers this for the vault method) — an assertion
  // addressed to one registered credential's id, but signed by a
  // *different* registered credential's private key, must be rejected
  // even though both credentials are individually valid and registered.
  it("an assertion for one credential id signed by a different registered credential's key is rejected", async () => {
    const alice = await generateWebAuthnKeyPair();
    const aliceCredentialId = new Uint8Array([1, 1, 1, 1]);
    const aliceRegOptions = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-options",
      payload: { userName: "alice" },
    });
    const aliceRegResponse = await buildRegistrationResponse(
      alice,
      (aliceRegOptions.json() as { challenge: string }).challenge,
      aliceCredentialId,
    );
    await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-verify",
      payload: { response: aliceRegResponse },
    });

    const bob = await generateWebAuthnKeyPair();
    const bobCredentialId = new Uint8Array([2, 2, 2, 2]);
    const bobRegOptions = await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-options",
      payload: { userName: "bob" },
    });
    const bobRegResponse = await buildRegistrationResponse(
      bob,
      (bobRegOptions.json() as { challenge: string }).challenge,
      bobCredentialId,
    );
    await app.inject({
      method: "POST",
      url: "/auth/webauthn/register-verify",
      payload: { response: bobRegResponse },
    });

    const loginOptions = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-options",
    });
    const { challenge } = loginOptions.json() as { challenge: string };
    // Claims Alice's credential id, but signs with Bob's private key.
    const forgedResponse = await buildAuthenticationResponse(
      bob.privateKey,
      challenge,
      aliceCredentialId,
    );

    const response = await app.inject({
      method: "POST",
      url: "/auth/webauthn/login-verify",
      payload: { response: forgedResponse },
    });
    expect(response.statusCode).toBe(401);
    expect(response.json()).toEqual({ error: "signature_invalid" });
    expect(response.headers["set-cookie"]).toBeUndefined();
  });
});

describe("Cross-account access", () => {
  it("a session for one identity cannot read another identity's account", async () => {
    const { app, mineKeyPair } = await buildTestApp();
    const { cookie } = await vaultSignIn(app, mineKeyPair);

    const sid = { sid: cookie.split("=")[1]! };
    const own = await app.inject({
      method: "GET",
      url: "/accounts/acc-mine/binance-connection",
      cookies: sid,
    });
    expect(own.statusCode).not.toBe(403);

    const crossAccess = await app.inject({
      method: "GET",
      url: "/accounts/acc-theirs/binance-connection",
      cookies: sid,
    });
    expect(crossAccess.statusCode).toBe(403);
    expect(crossAccess.json()).toEqual({
      error: "cross_account_access_denied",
    });

    await app.close();
  });
});

describe("Cross-site requests", () => {
  const change = {
    method: "PUT" as const,
    url: "/profile/settings",
    payload: { privacyMode: true },
  };
  const APP_ORIGIN = "https://app.example.test";

  it("a state-changing request from an origin the API does not serve is refused", async () => {
    const { app, mineKeyPair } = await buildTestApp({
      corsOrigins: [APP_ORIGIN],
    });
    const { cookie } = await vaultSignIn(app, mineKeyPair);

    const response = await app.inject({
      ...change,
      cookies: { sid: cookie.split("=")[1]! },
      headers: { origin: "https://evil.example.test" },
    });
    expect(response.statusCode).toBe(403);
    expect(response.json()).toEqual({ error: "origin_not_allowed" });

    await app.close();
  });

  it("the same change from the web app's own origin goes through", async () => {
    const { app, mineKeyPair } = await buildTestApp({
      corsOrigins: [APP_ORIGIN],
    });
    const { cookie } = await vaultSignIn(app, mineKeyPair);

    const response = await app.inject({
      ...change,
      cookies: { sid: cookie.split("=")[1]! },
      headers: { origin: APP_ORIGIN },
    });
    expect(response.statusCode).toBe(200);

    await app.close();
  });

  it("the session cookie is SameSite=Lax, HttpOnly and Secure when served over HTTPS", async () => {
    const { app, mineKeyPair } = await buildTestApp({ secureCookies: true });
    const { verifyResponse } = await vaultSignIn(app, mineKeyPair);

    const setCookie = String(verifyResponse.headers["set-cookie"]);
    expect(setCookie).toMatch(/SameSite=Lax/i);
    expect(setCookie).toMatch(/HttpOnly/i);
    expect(setCookie).toMatch(/Secure/i);
    expect(setCookie).toMatch(/Max-Age=2592000/i);

    await app.close();
  });
});

describe("Payload and rate limits", () => {
  it("an oversized payload is rejected", async () => {
    // Large enough for the vault sign-in payload to pass, small enough
    // to reject the oversized body below.
    const { app, mineKeyPair } = await buildTestApp({ bodyLimitBytes: 1024 });
    const { cookie } = await vaultSignIn(app, mineKeyPair);

    const response = await app.inject({
      method: "PUT",
      url: "/profile/settings",
      cookies: { sid: cookie.split("=")[1]! },
      payload: { privacyMode: true, padding: "x".repeat(5000) },
    });
    expect(response.statusCode).toBe(413);

    await app.close();
  });

  it("exceeding the proof quota blocks further proofs before they start", async () => {
    const { app, mineKeyPair } = await buildTestApp({
      proofRateLimiter: new RateLimiter(2, 60_000),
    });
    const { cookie } = await vaultSignIn(app, mineKeyPair);
    const address = expectedSubjectKey(mineKeyPair.qx, mineKeyPair.qy);

    const attempt = () =>
      app.inject({
        method: "GET",
        url: `/accounts/${address}/binance-performance-proof`,
        cookies: { sid: cookie.split("=")[1]! },
      });

    const first = await attempt();
    const second = await attempt();
    const third = await attempt();

    expect(first.statusCode).not.toBe(429);
    expect(second.statusCode).not.toBe(429);
    expect(third.statusCode).toBe(429);
    expect(third.json()).toEqual({ error: "rate_limited" });

    await app.close();
  });
});
