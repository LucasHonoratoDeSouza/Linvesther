import { describe, expect, it } from "vitest";
import { decryptPrivateExport, encryptPrivateExport } from "../src/exports/privateExport.js";
import { buildPublicBundle, verifyBundleHashes } from "../src/exports/publicBundle.js";
import type { BundleSourceData } from "../src/exports/types.js";

function source(overrides: Partial<BundleSourceData> = {}): BundleSourceData {
  return {
    identity: { identityId: "id-1", owner: "0xabc" },
    accounts: { members: ["acc-1"] },
    checkpoints: [{ sequence: 0, hash: "0x01" }],
    policies: [{ name: "spot-v1" }],
    attestations: [{ mechanism: "A0" }],
    proofs: [{ receipt: "0xreceipt", journal: "0xjournal" }],
    anchors: [{ txHash: "0xtx", blockNumber: 5 }],
    corrections: [],
    witness: { rawTrades: ["secret-trade-1"] },
    apiKeys: ["sk-live-deadbeef"],
    rawPrivateSeries: [{ balance: 1_000_000 }],
    privateSalts: ["salt-abc"],
    sourceNamespaceUids: ["binance:uid:998877"],
    ...overrides,
  };
}

describe("buildPublicBundle", () => {
  it("includes exactly the files named in commitments.md's bundle layout", () => {
    const bundle = buildPublicBundle(source());
    expect(Object.keys(bundle.files).sort()).toEqual(
      ["anchors/index.json", "accounts.json", "attestations/index.json", "checkpoints/index.json", "corrections/index.json", "identity.json", "manifest.json", "policies/index.json", "proofs/index.json"].sort(),
    );
  });

  it("never includes the witness, API keys, raw private series, salts or source UIDs anywhere in the output", () => {
    const src = source();
    const bundle = buildPublicBundle(src);
    const serialized = Object.values(bundle.files).join("\n");

    expect(serialized).not.toContain("secret-trade-1");
    expect(serialized).not.toContain("sk-live-deadbeef");
    expect(serialized).not.toContain("1000000"); // rawPrivateSeries balance
    expect(serialized).not.toContain("salt-abc");
    expect(serialized).not.toContain("binance:uid:998877");
    expect(serialized).not.toContain("witness");
    expect(serialized).not.toContain("apiKeys");
    expect(serialized).not.toContain("rawPrivateSeries");
    expect(serialized).not.toContain("privateSalts");
    expect(serialized).not.toContain("sourceNamespaceUids");
  });

  it("the manifest's hashes verify against the bundle's own files", () => {
    const bundle = buildPublicBundle(source());
    expect(verifyBundleHashes(bundle)).toEqual([]);
  });

  it("detects a tampered file against the manifest", () => {
    const bundle = buildPublicBundle(source());
    const tampered = { ...bundle, files: { ...bundle.files, "identity.json": JSON.stringify({ identityId: "id-2" }) } };
    const mismatches = verifyBundleHashes(tampered);
    expect(mismatches).toHaveLength(1);
    expect(mismatches[0]!.path).toBe("identity.json");
  });

  it("detects a file missing from the bundle despite being manifested", () => {
    const bundle = buildPublicBundle(source());
    const { "identity.json": _removed, ...remainingFiles } = bundle.files;
    const broken = { ...bundle, files: remainingFiles };
    const mismatches = verifyBundleHashes(broken);
    expect(mismatches).toEqual([{ path: "identity.json", expected: bundle.manifest.files["identity.json"]!.hash, actual: "<missing file>" }]);
  });
});

describe("private export encryption", () => {
  const payload = { witness: { rawTrades: ["t1"] }, rawPrivateSeries: [{ balance: 500 }], privateSalts: ["s1"] };

  it("round-trips exactly with the correct passphrase — the portability guarantee", () => {
    const encrypted = encryptPrivateExport(payload, "correct horse battery staple");
    const decrypted = decryptPrivateExport(encrypted, "correct horse battery staple");
    expect(decrypted).toEqual(payload);
  });

  it("the ciphertext never contains the plaintext witness data", () => {
    const encrypted = encryptPrivateExport(payload, "correct horse battery staple");
    expect(encrypted.ciphertext).not.toContain("rawTrades");
    expect(Buffer.from(encrypted.ciphertext, "base64").toString("latin1")).not.toContain("t1");
  });

  it("rejects the wrong passphrase instead of returning corrupted data", () => {
    const encrypted = encryptPrivateExport(payload, "correct horse battery staple");
    expect(() => decryptPrivateExport(encrypted, "wrong passphrase")).toThrow();
  });

  it("rejects a tampered ciphertext", () => {
    const encrypted = encryptPrivateExport(payload, "correct horse battery staple");
    const tamperedBytes = Buffer.from(encrypted.ciphertext, "base64");
    tamperedBytes[0] = tamperedBytes[0]! ^ 0xff;
    const tampered = { ...encrypted, ciphertext: tamperedBytes.toString("base64") };
    expect(() => decryptPrivateExport(tampered, "correct horse battery staple")).toThrow();
  });
});
