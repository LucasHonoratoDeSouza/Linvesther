import { createHash } from "node:crypto";
import type { BundleFiles, BundleManifest, BundleSourceData, PublicBundle } from "./types.js";

function sha256Hex(content: string): string {
  return createHash("sha256").update(content, "utf8").digest("hex");
}

/** Builds the public bundle by naming exactly the safe files (identity,
 * accounts, checkpoints, policies, attestations, proofs, anchors,
 * corrections) — never by copying `source` and deleting the unsafe
 * fields. `source.witness`/`apiKeys`/`rawPrivateSeries`/`privateSalts`/
 * `sourceNamespaceUids` are simply never read here, so a new private
 * field added to `BundleSourceData` later cannot leak silently; it would
 * have to be wired in on purpose. */
export function buildPublicBundle(source: BundleSourceData): PublicBundle {
  const files: BundleFiles = {
    "identity.json": JSON.stringify(source.identity),
    "accounts.json": JSON.stringify(source.accounts),
    "checkpoints/index.json": JSON.stringify(source.checkpoints),
    "policies/index.json": JSON.stringify(source.policies),
    "attestations/index.json": JSON.stringify(source.attestations),
    "proofs/index.json": JSON.stringify(source.proofs),
    "anchors/index.json": JSON.stringify(source.anchors),
    "corrections/index.json": JSON.stringify(source.corrections),
  };

  const manifestFiles: BundleManifest["files"] = {};
  for (const [path, content] of Object.entries(files)) {
    manifestFiles[path] = { hash: sha256Hex(content), sizeBytes: Buffer.byteLength(content, "utf8") };
  }
  const manifest: BundleManifest = { version: "1", files: manifestFiles };

  return { manifest, files: { ...files, "manifest.json": JSON.stringify(manifest) } };
}

export interface HashMismatch {
  path: string;
  expected: string;
  actual: string;
}

/** Recomputes every manifested file's hash from its actual content and
 * compares it to what the manifest claims — the "hashes verificáveis"
 * requirement. Returns an empty array when everything matches. */
export function verifyBundleHashes(bundle: PublicBundle): HashMismatch[] {
  const mismatches: HashMismatch[] = [];
  for (const [path, entry] of Object.entries(bundle.manifest.files)) {
    const content = bundle.files[path];
    if (content === undefined) {
      mismatches.push({ path, expected: entry.hash, actual: "<missing file>" });
      continue;
    }
    const actual = sha256Hex(content);
    if (actual !== entry.hash) {
      mismatches.push({ path, expected: entry.hash, actual });
    }
  }
  return mismatches;
}
