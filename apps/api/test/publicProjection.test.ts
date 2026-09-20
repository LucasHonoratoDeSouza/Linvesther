import { describe, expect, it } from "vitest";
import type { InternalTrackRecord } from "../src/public/internal.js";
import { toPublicProjection } from "../src/public/projection.js";

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
    gaps: [{ startMs: 1000, endMs: 2000 }],
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

describe("toPublicProjection", () => {
  it("keeps the minimal profile visible even when every dimension is unavailable", () => {
    const record = internalRecord({
      coverageStatus: "UNAVAILABLE",
      calculationStatus: "UNPROVEN",
      registryStatus: "UNANCHORED",
      availabilityStatus: "UNAVAILABLE",
      verifiedSince: null,
    });
    const projection = toPublicProjection(record);
    expect(projection.profile).toEqual({ identityId: "id-1", trackId: "track-1", createdAt: "2026-01-01T00:00:00Z", verifiedSince: null, currency: "USDT" });
  });

  it("exposes all five dimensions separately", () => {
    const projection = toPublicProjection(internalRecord());
    expect(projection.dimensions).toEqual({
      origin: "A0",
      coverage: "POLICY_COMPLETE",
      calculation: "ZK_VERIFIED",
      registry: "FINALIZED",
      availability: "AVAILABLE",
    });
  });

  it("carries gaps and corrections through unchanged", () => {
    const record = internalRecord();
    const projection = toPublicProjection(record);
    expect(projection.gaps).toEqual(record.gaps);
    expect(projection.corrections).toEqual(record.corrections);
  });

  it("never includes any private field, by key or by value, anywhere in the output", () => {
    const record = internalRecord();
    const projection = toPublicProjection(record);
    const serialized = JSON.stringify(projection);

    const forbiddenKeys = ["exactBalanceUsd", "operations", "sourceApiKeyDigest", "institutionWalletId", "witnessBlob", "sourceNamespaceUid"];
    for (const key of forbiddenKeys) {
      expect(Object.keys(projection)).not.toContain(key);
      expect(serialized).not.toContain(key);
    }

    const forbiddenValues = [record.exactBalanceUsd, record.sourceApiKeyDigest, record.institutionWalletId, record.witnessBlob, record.sourceNamespaceUid];
    for (const value of forbiddenValues) {
      expect(serialized).not.toContain(value);
    }
  });
});
