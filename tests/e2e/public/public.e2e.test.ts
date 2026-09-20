import { buildApp, MemoryPublicTrackStore, type InternalTrackRecord } from "@linvestherzk/api";
import { describe, expect, it } from "vitest";

// End-to-end, per the acceptance criteria: perfil mínimo continua visível;
// cinco dimensões, gaps e correções corretos; nenhum UID/raw/segredo
// vaza. Driven through the real Fastify app via `.inject()` — the
// route's actual JSON response is what gets scanned for leaks, not an
// internal object.

const DOMAIN = "app.linvestherzk.example";

function internalRecord(overrides: Partial<InternalTrackRecord> = {}): InternalTrackRecord {
  return {
    identityId: "id-1",
    trackId: "track-1",
    createdAt: "2026-01-01T00:00:00Z",
    verifiedSince: "2026-01-05T00:00:00Z",
    currency: "USDT",
    originMechanism: "A0",
    coverageStatus: "POLICY_COMPLETE",
    calculationStatus: "ZK_VERIFIED",
    registryStatus: "FINALIZED",
    availabilityStatus: "AVAILABLE",
    gaps: [{ startMs: 1_000, endMs: 2_000 }],
    corrections: [{ targetDigest: "0xabc", reasonCode: "bad-price-feed", effectiveFrom: "2026-01-10T00:00:00Z" }],
    exactBalanceUsd: "1234567.89",
    operations: [{ symbol: "BTCUSDT", side: "buy", quantity: "0.5" }],
    sourceApiKeyDigest: "sha256:deadbeef",
    institutionWalletId: "binance-wallet-42",
    witnessBlob: "top-secret-witness-bytes",
    sourceNamespaceUid: "binance:uid:998877",
    ...overrides,
  };
}

describe("Public track projection", () => {
  it("is readable without authentication and matches the five-dimension shape, with gaps/corrections intact", async () => {
    const trackStore = new MemoryPublicTrackStore();
    trackStore.save(internalRecord());
    const app = buildApp({ domain: DOMAIN, publicTrackStore: trackStore });

    const response = await app.inject({ method: "GET", url: "/public/tracks/track-1" });
    expect(response.statusCode).toBe(200);
    const body = response.json();

    expect(body.profile).toEqual({ identityId: "id-1", trackId: "track-1", createdAt: "2026-01-01T00:00:00Z", verifiedSince: "2026-01-05T00:00:00Z", currency: "USDT" });
    expect(body.dimensions).toEqual({ origin: "A0", coverage: "POLICY_COMPLETE", calculation: "ZK_VERIFIED", registry: "FINALIZED", availability: "AVAILABLE" });
    expect(body.gaps).toEqual([{ startMs: 1_000, endMs: 2_000 }]);
    expect(body.corrections).toEqual([{ targetDigest: "0xabc", reasonCode: "bad-price-feed", effectiveFrom: "2026-01-10T00:00:00Z" }]);

    await app.close();
  });

  it("keeps the minimal profile visible even when every dimension is unavailable", async () => {
    const trackStore = new MemoryPublicTrackStore();
    trackStore.save(
      internalRecord({
        coverageStatus: "UNAVAILABLE",
        calculationStatus: "UNPROVEN",
        registryStatus: "UNANCHORED",
        availabilityStatus: "UNAVAILABLE",
        verifiedSince: null,
        gaps: [],
        corrections: [],
      }),
    );
    const app = buildApp({ domain: DOMAIN, publicTrackStore: trackStore });

    const response = await app.inject({ method: "GET", url: "/public/tracks/track-1" });
    expect(response.statusCode).toBe(200);
    expect(response.json().profile).toEqual({ identityId: "id-1", trackId: "track-1", createdAt: "2026-01-01T00:00:00Z", verifiedSince: null, currency: "USDT" });

    await app.close();
  });

  it("never leaks a UID, raw operation, or secret in the raw HTTP response body", async () => {
    const trackStore = new MemoryPublicTrackStore();
    const record = internalRecord();
    trackStore.save(record);
    const app = buildApp({ domain: DOMAIN, publicTrackStore: trackStore });

    const response = await app.inject({ method: "GET", url: "/public/tracks/track-1" });
    const rawBody = response.body;

    for (const secret of [record.exactBalanceUsd, record.sourceApiKeyDigest, record.institutionWalletId, record.witnessBlob, record.sourceNamespaceUid, "BTCUSDT"]) {
      expect(rawBody).not.toContain(secret);
    }
    for (const key of ["exactBalanceUsd", "operations", "sourceApiKeyDigest", "institutionWalletId", "witnessBlob", "sourceNamespaceUid"]) {
      expect(rawBody).not.toContain(key);
    }

    await app.close();
  });
});
