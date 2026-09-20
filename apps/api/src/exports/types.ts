// Bundle export, per the protocol specification's "Bundle
// público portátil": "manifest.json, identity.json, accounts.json,
// checkpoints/, policies/, attestations/, proofs/, anchors/,
// corrections/... Nunca incluir API keys, raw privado, salts privados,
// UID, série privada ou witness. Exportação privada do titular é outro
// artefato cifrado, opcional."

export interface BundleManifestEntry {
  hash: string;
  sizeBytes: number;
}

export interface BundleManifest {
  version: "1";
  files: Record<string, BundleManifestEntry>;
}

/** File path -> raw content (each file is stored as its own string, not
 * pre-parsed, matching how they'd actually be written to a directory). */
export type BundleFiles = Record<string, string>;

export interface PublicBundle {
  manifest: BundleManifest;
  files: BundleFiles;
}

/** Everything a bundle draws from, including the fields the public
 * bundle must never contain. Kept as one source type — mirroring
 * `InternalTrackRecord` — so the "never leaks" tests have something real
 * to check against, not an already-safe fixture. */
export interface BundleSourceData {
  identity: Record<string, unknown>;
  accounts: Record<string, unknown>;
  checkpoints: Record<string, unknown>[];
  policies: Record<string, unknown>[];
  attestations: Record<string, unknown>[];
  proofs: Record<string, unknown>[];
  anchors: Record<string, unknown>[];
  corrections: Record<string, unknown>[];

  // --- private data that must never reach the public bundle ---
  witness: Record<string, unknown>;
  apiKeys: string[];
  rawPrivateSeries: Record<string, unknown>[];
  privateSalts: string[];
  sourceNamespaceUids: string[];
}
