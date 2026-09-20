import { concat, getAddress, keccak256, slice } from "viem";
import { describe, expect, it } from "vitest";
import { claimSetDigest } from "../src/claims/claimSet.js";
import { checkAudience, checkExpiry, verifyConsent } from "../src/claims/disclosure.js";
import type { ClaimSet } from "../src/claims/types.js";
import { credentialStore } from "../src/auth/identitySignature.js";

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
}

function subjectKeyOf(qx: `0x${string}`, qy: `0x${string}`): `0x${string}` {
  return getAddress(slice(keccak256(concat([qx, qy])), 12));
}

async function registerOwner() {
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  const qx = bytesToHex(raw.slice(1, 33));
  const qy = bytesToHex(raw.slice(33, 65));
  const subjectKey = subjectKeyOf(qx, qy);
  credentialStore.register(subjectKey, qx, qy, "vault");
  return { privateKey, subjectKey };
}

async function signDigest(privateKey: CryptoKey, digest: string): Promise<`0x${string}`> {
  const signature = await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, new TextEncoder().encode(digest));
  return bytesToHex(new Uint8Array(signature));
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

async function sign(privateKey: CryptoKey, claimSet: ClaimSet) {
  return signDigest(privateKey, claimSetDigest(claimSet));
}

describe("verifyConsent", () => {
  it("accepts a well-formed, correctly signed, unexpired ClaimSet", async () => {
    const { privateKey, subjectKey } = await registerOwner();
    const claimSet = baseClaimSet();
    const signature = await sign(privateKey, claimSet);
    const result = await verifyConsent(claimSet, { method: "vault", signature }, subjectKey, new Date("2026-01-15T00:00:00Z"));
    expect(result.ok).toBe(true);
  });

  it("rejects an expired ClaimSet", async () => {
    const { privateKey, subjectKey } = await registerOwner();
    const claimSet = baseClaimSet();
    const signature = await sign(privateKey, claimSet);
    const result = await verifyConsent(claimSet, { method: "vault", signature }, subjectKey, new Date("2026-02-02T00:00:00Z"));
    expect(result).toEqual({ ok: false, error: "expired" });
  });

  it("changing the checkpoint after signing invalidates consent", async () => {
    const { privateKey, subjectKey } = await registerOwner();
    const claimSet = baseClaimSet();
    const signature = await sign(privateKey, claimSet);
    const tampered = { ...claimSet, checkpointId: "cp-99" };
    const result = await verifyConsent(tampered, { method: "vault", signature }, subjectKey, new Date("2026-01-15T00:00:00Z"));
    expect(result).toEqual({ ok: false, error: "signature_invalid" });
  });

  it("changing the period after signing invalidates consent", async () => {
    const { privateKey, subjectKey } = await registerOwner();
    const claimSet = baseClaimSet();
    const signature = await sign(privateKey, claimSet);
    const tampered = { ...claimSet, periodEnd: "2026-06-30T00:00:00Z" };
    const result = await verifyConsent(tampered, { method: "vault", signature }, subjectKey, new Date("2026-01-15T00:00:00Z"));
    expect(result).toEqual({ ok: false, error: "signature_invalid" });
  });

  it("changing a claim predicate after signing invalidates consent", async () => {
    const { privateKey, subjectKey } = await registerOwner();
    const claimSet = baseClaimSet();
    const signature = await sign(privateKey, claimSet);
    const tampered = { ...claimSet, claims: [{ type: "GE" as const, metric: "twr", threshold: "999" }] };
    const result = await verifyConsent(tampered, { method: "vault", signature }, subjectKey, new Date("2026-01-15T00:00:00Z"));
    expect(result).toEqual({ ok: false, error: "signature_invalid" });
  });

  it("changing the audience after signing invalidates consent", async () => {
    const { privateKey, subjectKey } = await registerOwner();
    const claimSet = baseClaimSet({ audience: "verifier-a" });
    const signature = await sign(privateKey, claimSet);
    const tampered = { ...claimSet, audience: "public" };
    const result = await verifyConsent(tampered, { method: "vault", signature }, subjectKey, new Date("2026-01-15T00:00:00Z"));
    expect(result).toEqual({ ok: false, error: "signature_invalid" });
  });

  it("rejects a signature produced by a different identity's key than the presented subjectKey", async () => {
    const { subjectKey } = await registerOwner();
    const stranger = await registerOwner();
    const claimSet = baseClaimSet();
    const signature = await sign(stranger.privateKey, claimSet);
    const result = await verifyConsent(claimSet, { method: "vault", signature }, subjectKey, new Date("2026-01-15T00:00:00Z"));
    expect(result).toEqual({ ok: false, error: "signature_invalid" });
  });

  it("rejects a subjectKey with no registered credential", async () => {
    const { privateKey } = await registerOwner();
    const unregisteredSubjectKey = "0x0000000000000000000000000000000000dEaD" as const;
    const claimSet = baseClaimSet();
    const signature = await sign(privateKey, claimSet);
    const result = await verifyConsent(claimSet, { method: "vault", signature }, unregisteredSubjectKey, new Date("2026-01-15T00:00:00Z"));
    expect(result).toEqual({ ok: false, error: "signature_invalid" });
  });
});

describe("checkExpiry", () => {
  it("an unexpired ClaimSet is not flagged", () => {
    const claimSet = baseClaimSet({ expiresAt: "2026-02-01T00:00:00Z" });
    expect(checkExpiry(claimSet, new Date("2026-01-15T00:00:00Z"))).toBeNull();
  });

  it("a ClaimSet at or past its expiresAt is flagged", () => {
    const claimSet = baseClaimSet({ expiresAt: "2026-02-01T00:00:00Z" });
    expect(checkExpiry(claimSet, new Date("2026-02-01T00:00:00Z"))).toBe("expired");
  });
});

describe("checkAudience", () => {
  it("a public audience is reusable by any requester", () => {
    const claimSet = baseClaimSet({ audience: "public" });
    expect(checkAudience(claimSet, "anyone")).toBeNull();
    expect(checkAudience(claimSet, "")).toBeNull();
  });

  it("a specific audience only matches the exact requester", () => {
    const claimSet = baseClaimSet({ audience: "verifier-a" });
    expect(checkAudience(claimSet, "verifier-a")).toBeNull();
    expect(checkAudience(claimSet, "verifier-b")).toBe("audience_mismatch");
  });
});
