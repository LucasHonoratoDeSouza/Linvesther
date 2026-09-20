import {
  buildApp,
  claimSetDigest,
  MemoryDisclosureStore,
  type ClaimSet,
} from "@linvestherzk/api";
import { afterEach, describe, expect, it } from "vitest";

// End-to-end, per the acceptance criteria: claims ligam checkpoint e período;
// disclosure exige consentimento do conteúdo exato e expiração/audience
// corretas. Driven through the real Fastify app via `.inject()`.

const DOMAIN = "app.linvestherzk.example";
const NOW = new Date("2026-01-15T00:00:00Z");

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
}

function baseClaimSet(overrides: Partial<ClaimSet> = {}): ClaimSet {
  return {
    identityId: "id-1",
    trackId: "track-1",
    checkpointId: "cp-42",
    periodStart: "2026-01-01T00:00:00Z",
    periodEnd: "2026-01-31T00:00:00Z",
    claims: [{ type: "GE", metric: "twr", threshold: "0.10" }],
    audience: "public",
    nonce: "n-1",
    expiresAt: "2026-02-01T00:00:00Z",
    ...overrides,
  };
}

async function buildTestApp(
  overrides: Partial<Parameters<typeof buildApp>[0]> = {},
) {
  const app = buildApp({
    domain: DOMAIN,
    now: () => NOW,
    currentClaimMetrics: async () => ({
      metrics: { twr: 0.5 },
      since: "2026-01-01T00:00:00.000Z",
      computedAt: NOW.toISOString(),
    }),
    ...overrides,
  });
  await app.ready();
  return { app };
}

interface Owner {
  privateKey: CryptoKey;
  qx: `0x${string}`;
  qy: `0x${string}`;
}

async function generateOwner(): Promise<Owner> {
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

/** Signs in via the vault ceremony — the same credential this returns is
 * reused to sign disclosure consent below, since /auth/vault/verify
 * auto-registers it on first use. */
async function signIn(
  app: Awaited<ReturnType<typeof buildTestApp>>["app"],
  owner: Owner,
) {
  const challengeResponse = await app.inject({
    method: "POST",
    url: "/auth/vault/challenge",
  });
  const { challenge } = challengeResponse.json() as { challenge: string };
  const signature = bytesToHex(
    new Uint8Array(
      await crypto.subtle.sign(
        { name: "ECDSA", hash: "SHA-256" },
        owner.privateKey,
        new TextEncoder().encode(challenge),
      ),
    ),
  );
  const verifyResponse = await app.inject({
    method: "POST",
    url: "/auth/vault/verify",
    payload: { qx: owner.qx, qy: owner.qy, challenge, signature },
  });
  const setCookie = verifyResponse.headers["set-cookie"];
  const cookie = Array.isArray(setCookie) ? setCookie[0] : setCookie;
  return cookie!.split(";")[0]!.split("=")[1]!;
}

async function signConsent(
  owner: Owner,
  claimSet: ClaimSet,
): Promise<{ method: "vault"; signature: `0x${string}` }> {
  const signature = bytesToHex(
    new Uint8Array(
      await crypto.subtle.sign(
        { name: "ECDSA", hash: "SHA-256" },
        owner.privateKey,
        new TextEncoder().encode(claimSetDigest(claimSet)),
      ),
    ),
  );
  return { method: "vault", signature };
}

describe("Disclosure: exact-content consent, checkpoint/period binding, expiration and audience", () => {
  it("publishes a well-formed, correctly signed disclosure and reads it back", async () => {
    const { app } = await buildTestApp();
    const owner = await generateOwner();
    const sid = await signIn(app, owner);
    const claimSet = baseClaimSet();
    const credential = await signConsent(owner, claimSet);

    const publish = await app.inject({
      method: "POST",
      url: "/claims/disclose",
      cookies: { sid },
      payload: { claimSet, credential },
    });
    expect(publish.statusCode).toBe(201);
    const { digest } = publish.json() as { digest: string };
    expect(digest).toBe(claimSetDigest(claimSet));

    const read = await app.inject({ method: "GET", url: `/claims/${digest}` });
    expect(read.statusCode).toBe(200);
    expect(read.json()).toEqual(claimSet);

    await app.close();
  });

  it("a disclosure whose checkpoint was changed after signing is rejected at publish time", async () => {
    const { app } = await buildTestApp();
    const owner = await generateOwner();
    const sid = await signIn(app, owner);
    const claimSet = baseClaimSet();
    const credential = await signConsent(owner, claimSet);
    const tampered = { ...claimSet, checkpointId: "cp-99" };

    const publish = await app.inject({
      method: "POST",
      url: "/claims/disclose",
      cookies: { sid },
      payload: { claimSet: tampered, credential },
    });
    expect(publish.statusCode).toBe(400);
    expect(publish.json()).toEqual({ error: "signature_invalid" });

    await app.close();
  });

  it("a disclosure whose period was changed after signing is rejected at publish time", async () => {
    const { app } = await buildTestApp();
    const owner = await generateOwner();
    const sid = await signIn(app, owner);
    const claimSet = baseClaimSet();
    const credential = await signConsent(owner, claimSet);
    const tampered = { ...claimSet, periodStart: "2020-01-01T00:00:00Z" };

    const publish = await app.inject({
      method: "POST",
      url: "/claims/disclose",
      cookies: { sid },
      payload: { claimSet: tampered, credential },
    });
    expect(publish.statusCode).toBe(400);
    expect(publish.json()).toEqual({ error: "signature_invalid" });

    await app.close();
  });

  it("an expired disclosure is rejected on read even though it was validly published earlier", async () => {
    const { app: publishApp } = await buildTestApp({
      now: () => new Date("2026-01-15T00:00:00Z"),
    });
    const owner = await generateOwner();
    const sid = await signIn(publishApp, owner);
    const claimSet = baseClaimSet({ expiresAt: "2026-01-20T00:00:00Z" });
    const credential = await signConsent(owner, claimSet);
    const publish = await publishApp.inject({
      method: "POST",
      url: "/claims/disclose",
      cookies: { sid },
      payload: { claimSet, credential },
    });
    const { digest } = publish.json() as { digest: string };
    await publishApp.close();

    // A later read, after expiresAt, against a fresh app instance
    // sharing the same disclosure store, sees it as expired.
    const disclosureStore = new MemoryDisclosureStore();
    disclosureStore.save(digest, {
      claimSet,
      credential,
      owner: "0x0000000000000000000000000000000000dEaD",
    });
    const { app: readApp } = await buildTestApp({
      now: () => new Date("2026-01-21T00:00:00Z"),
      disclosureStore,
    });
    const read = await readApp.inject({
      method: "GET",
      url: `/claims/${digest}`,
    });
    expect(read.statusCode).toBe(410);
    expect(read.json()).toEqual({ error: "expired" });

    await readApp.close();
  });

  it("a restricted-audience disclosure is rejected for the wrong audience and accepted for the right one", async () => {
    const { app } = await buildTestApp();
    const owner = await generateOwner();
    const sid = await signIn(app, owner);
    const claimSet = baseClaimSet({ audience: "verifier-a" });
    const credential = await signConsent(owner, claimSet);
    const publish = await app.inject({
      method: "POST",
      url: "/claims/disclose",
      cookies: { sid },
      payload: { claimSet, credential },
    });
    const { digest } = publish.json() as { digest: string };

    const wrongAudience = await app.inject({
      method: "GET",
      url: `/claims/${digest}?audience=verifier-b`,
    });
    expect(wrongAudience.statusCode).toBe(403);

    const rightAudience = await app.inject({
      method: "GET",
      url: `/claims/${digest}?audience=verifier-a`,
    });
    expect(rightAudience.statusCode).toBe(200);

    await app.close();
  });
});
