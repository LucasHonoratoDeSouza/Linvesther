import { buildApp, MemoryEraStore } from "@linvestherzk/api";
import { afterEach, describe, expect, it } from "vitest";

// End-to-end, per the acceptance criteria: a new era preserves lifetime and
// the declared label; a cutout/excerpt never replaces the entire
// history. Driven through the real Fastify app via `.inject()`.

const DOMAIN = "app.linvestherzk.example";
const NOW = new Date("2026-01-01T00:05:00Z");

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
}

async function buildTestApp() {
  const eraStore = new MemoryEraStore();
  const app = buildApp({ domain: DOMAIN, now: () => NOW, eraStore });
  await app.ready();
  return { app, eraStore };
}

async function signIn(app: Awaited<ReturnType<typeof buildTestApp>>["app"]) {
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  const qx = bytesToHex(raw.slice(1, 33));
  const qy = bytesToHex(raw.slice(33, 65));

  const challengeResponse = await app.inject({ method: "POST", url: "/auth/vault/challenge" });
  const { challenge } = challengeResponse.json() as { challenge: string };
  const signature = bytesToHex(
    new Uint8Array(await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, new TextEncoder().encode(challenge))),
  );

  const verifyResponse = await app.inject({ method: "POST", url: "/auth/vault/verify", payload: { qx, qy, challenge, signature } });
  const setCookie = verifyResponse.headers["set-cookie"];
  const cookie = Array.isArray(setCookie) ? setCookie[0] : setCookie;
  return cookie!.split(";")[0]!.split("=")[1]!;
}

describe("Declared strategy eras", () => {
  let app: Awaited<ReturnType<typeof buildTestApp>>["app"];

  afterEach(async () => {
    await app?.close();
  });

  it("declaring a new era preserves the track's lifetime start and the declared label", async () => {
    ({ app } = await buildTestApp());
    const sid = await signIn(app);

    const response = await app.inject({
      method: "POST",
      url: "/tracks/track-1/eras",
      cookies: { sid },
      payload: { label: "switched to a market-neutral basis strategy", description: "full description text", lifetimeStartMs: 1_000 },
    });
    expect(response.statusCode).toBe(201);
    const timeline = response.json() as { lifetimeStartMs: number; eras: { label: string }[] };
    expect(timeline.lifetimeStartMs).toBe(1_000);
    expect(timeline.eras).toHaveLength(1);
    expect(timeline.eras[0]!.label).toBe("switched to a market-neutral basis strategy");
  });

  it("a later era never moves the lifetime start already recorded for the track", async () => {
    ({ app } = await buildTestApp());
    const sid = await signIn(app);

    await app.inject({ method: "POST", url: "/tracks/track-1/eras", cookies: { sid }, payload: { label: "era A", description: "d1", lifetimeStartMs: 1_000 } });
    const second = await app.inject({ method: "POST", url: "/tracks/track-1/eras", cookies: { sid }, payload: { label: "era B", description: "d2", lifetimeStartMs: 999_999 } });

    const timeline = second.json() as { lifetimeStartMs: number; eras: unknown[] };
    expect(timeline.lifetimeStartMs).toBe(1_000);
    expect(timeline.eras).toHaveLength(2);
  });

  it("declaring an era without a session is rejected", async () => {
    ({ app } = await buildTestApp());
    const response = await app.inject({ method: "POST", url: "/tracks/track-1/eras", payload: { label: "era A", description: "d1", lifetimeStartMs: 1_000 } });
    expect(response.statusCode).toBe(401);
  });

  it("reading the full timeline requires no session and returns every declared era", async () => {
    ({ app } = await buildTestApp());
    const sid = await signIn(app);
    await app.inject({ method: "POST", url: "/tracks/track-1/eras", cookies: { sid }, payload: { label: "era A", description: "d1", lifetimeStartMs: 1_000 } });

    const response = await app.inject({ method: "GET", url: "/tracks/track-1/eras" });
    expect(response.statusCode).toBe(200);
    const timeline = response.json() as { lifetimeStartMs: number; eras: unknown[] };
    expect(timeline.lifetimeStartMs).toBe(1_000);
    expect(timeline.eras).toHaveLength(1);
  });

  it("an era-scoped excerpt is always marked isExcerpt and carries the real lifetimeStartMs — it never substitutes for the full lifetime", async () => {
    ({ app } = await buildTestApp());
    const sid = await signIn(app);
    const created = await app.inject({ method: "POST", url: "/tracks/track-1/eras", cookies: { sid }, payload: { label: "era A", description: "d1", lifetimeStartMs: 1_000 } });
    const eraId = (created.json() as { eras: { eraId: string }[] }).eras[0]!.eraId;

    const response = await app.inject({ method: "GET", url: `/tracks/track-1/eras/${eraId}` });
    expect(response.statusCode).toBe(200);
    const view = response.json() as { isExcerpt: boolean; lifetimeStartMs: number; era: { label: string } };
    expect(view.isExcerpt).toBe(true);
    expect(view.lifetimeStartMs).toBe(1_000);
    expect(view.era.label).toBe("era A");
  });

  it("an unknown era id is rejected rather than silently returning the full timeline", async () => {
    ({ app } = await buildTestApp());
    const sid = await signIn(app);
    await app.inject({ method: "POST", url: "/tracks/track-1/eras", cookies: { sid }, payload: { label: "era A", description: "d1", lifetimeStartMs: 1_000 } });

    const response = await app.inject({ method: "GET", url: "/tracks/track-1/eras/not-a-real-era-id" });
    expect(response.statusCode).toBe(404);
  });
});
