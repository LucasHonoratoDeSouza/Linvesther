import { buildApp, MemoryMultiAccountStore } from "@linvestherzk/api";
import { concat, getAddress, keccak256, slice } from "viem";
import { afterEach, describe, expect, it } from "vitest";

// End-to-end, per the acceptance criteria: duas contas são acompanhadas
// ponta a ponta com membership público, fee em trânsito e gap de um
// membro impedindo retorno agregado. Driven through the real Fastify
// app via `.inject()`.

const DOMAIN = "app.linvestherzk.example";
const NOW = new Date("2026-01-01T00:05:00Z");
const TRACK_ID = "track-1";

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
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  return { privateKey, qx: bytesToHex(raw.slice(1, 33)), qy: bytesToHex(raw.slice(33, 65)) };
}

function expectedSubjectKey(qx: `0x${string}`, qy: `0x${string}`): `0x${string}` {
  return getAddress(slice(keccak256(concat([qx, qy])), 12));
}

async function buildTestApp() {
  const ownerKeyPair = await generateVaultKeyPair();
  const multiAccountStore = new MemoryMultiAccountStore();
  const multiAccountTrackOwners = new Map<string, `0x${string}`>([[TRACK_ID, expectedSubjectKey(ownerKeyPair.qx, ownerKeyPair.qy)]]);
  const app = buildApp({ domain: DOMAIN, now: () => NOW, multiAccountStore, multiAccountTrackOwners });
  await app.ready();
  return { app, multiAccountStore, ownerKeyPair };
}

async function signIn(app: Awaited<ReturnType<typeof buildTestApp>>["app"], keyPair: VaultKeyPair) {
  const challengeResponse = await app.inject({ method: "POST", url: "/auth/vault/challenge" });
  const { challenge } = challengeResponse.json() as { challenge: string };
  const signature = bytesToHex(
    new Uint8Array(
      await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, keyPair.privateKey, new TextEncoder().encode(challenge)),
    ),
  );
  const verifyResponse = await app.inject({
    method: "POST",
    url: "/auth/vault/verify",
    payload: { qx: keyPair.qx, qy: keyPair.qy, challenge, signature },
  });
  const setCookie = verifyResponse.headers["set-cookie"];
  const cookie = Array.isArray(setCookie) ? setCookie[0] : setCookie;
  return cookie!.split(";")[0]!.split("=")[1]!;
}

async function addMember(app: Awaited<ReturnType<typeof buildTestApp>>["app"], sid: string, accountId: string, navMicros: number) {
  return app.inject({ method: "POST", url: `/tracks/${TRACK_ID}/members`, cookies: { sid }, payload: { accountId, navMicros } });
}

describe("Multi-account consolidation", () => {
  let app: Awaited<ReturnType<typeof buildTestApp>>["app"];
  let multiAccountStore: Awaited<ReturnType<typeof buildTestApp>>["multiAccountStore"];

  afterEach(async () => {
    await app?.close();
  });

  it("tracks two accounts end-to-end: membership is public and the aggregate sums both", async () => {
    let ownerKeyPair: VaultKeyPair;
    ({ app, multiAccountStore, ownerKeyPair } = await buildTestApp());
    const sid = await signIn(app, ownerKeyPair);

    await addMember(app, sid, "acc-a", 1_000_000);
    await addMember(app, sid, "acc-b", 500_000);

    const membershipResponse = await app.inject({ method: "GET", url: `/tracks/${TRACK_ID}/members` });
    expect(membershipResponse.statusCode).toBe(200);
    const membership = membershipResponse.json() as { members: { accountId: string }[] };
    expect(membership.members.map((m) => m.accountId)).toEqual(["acc-a", "acc-b"]);

    const aggregateResponse = await app.inject({ method: "GET", url: `/tracks/${TRACK_ID}/aggregate` });
    expect(aggregateResponse.statusCode).toBe(200);
    expect(aggregateResponse.json()).toEqual({ status: "available", navMicros: 1_500_000 });
    void multiAccountStore;
  });

  it("adding a member without a session is rejected — membership is publicly readable but not publicly writable", async () => {
    ({ app } = await buildTestApp());
    const response = await app.inject({ method: "POST", url: `/tracks/${TRACK_ID}/members`, payload: { accountId: "acc-a", navMicros: 100 } });
    expect(response.statusCode).toBe(401);
  });

  it("an internal transfer's fee shows up as a real NAV reduction in the aggregate", async () => {
    let ownerKeyPair: VaultKeyPair;
    ({ app, ownerKeyPair } = await buildTestApp());
    const sid = await signIn(app, ownerKeyPair);
    await addMember(app, sid, "acc-a", 1_000_000);
    await addMember(app, sid, "acc-b", 500_000);

    const transferResponse = await app.inject({
      method: "POST",
      url: `/tracks/${TRACK_ID}/transfers`,
      cookies: { sid },
      payload: { fromAccountId: "acc-a", toAccountId: "acc-b", outgoingMicros: 100_000, feeMicros: 1_000, incomingMicros: 99_000 },
    });
    expect(transferResponse.statusCode).toBe(201);

    const aggregateResponse = await app.inject({ method: "GET", url: `/tracks/${TRACK_ID}/aggregate` });
    // Total NAV started at 1,500,000; the 1,000-micro fee is a real
    // reduction, not absorbed silently: 1,000,000-100,000 + 500,000+99,000 = 1,499,000.
    expect(aggregateResponse.json()).toEqual({ status: "available", navMicros: 1_499_000 });
  });

  it("rejects an unreconciled transfer rather than silently accepting a mismatched fee", async () => {
    let ownerKeyPair: VaultKeyPair;
    ({ app, ownerKeyPair } = await buildTestApp());
    const sid = await signIn(app, ownerKeyPair);
    await addMember(app, sid, "acc-a", 1_000_000);
    await addMember(app, sid, "acc-b", 500_000);

    const response = await app.inject({
      method: "POST",
      url: `/tracks/${TRACK_ID}/transfers`,
      cookies: { sid },
      payload: { fromAccountId: "acc-a", toAccountId: "acc-b", outgoingMicros: 100_000, feeMicros: 1_000, incomingMicros: 100_000 },
    });
    expect(response.statusCode).toBe(400);
  });

  it("a gap on any one member blocks the aggregate return entirely, not just for that member", async () => {
    let ownerKeyPair: VaultKeyPair;
    ({ app, multiAccountStore, ownerKeyPair } = await buildTestApp());
    const sid = await signIn(app, ownerKeyPair);
    await addMember(app, sid, "acc-a", 1_000_000);
    await addMember(app, sid, "acc-b", 500_000);
    multiAccountStore.setGap(TRACK_ID, "acc-b", true);

    const aggregateResponse = await app.inject({ method: "GET", url: `/tracks/${TRACK_ID}/aggregate` });
    expect(aggregateResponse.statusCode).toBe(200);
    expect(aggregateResponse.json()).toEqual({ status: "unavailable", reason: "member_gap", accountId: "acc-b" });
  });

  it("a third party cannot add a member to someone else's track", async () => {
    ({ app } = await buildTestApp());
    const thirdParty = await generateVaultKeyPair();
    const sid = await signIn(app, thirdParty);
    const response = await addMember(app, sid, "acc-a", 1_000_000);
    expect(response.statusCode).toBe(403);
  });
});
