import { createHash } from "node:crypto";
import canonicalize from "canonicalize";
import type { ClaimSet } from "./types.js";

/** JCS canonical JSON of the whole ClaimSet — every field, including
 * `checkpointId`/`periodStart`/`periodEnd`, participates in the digest
 * that gets signed. Uses the same `canonicalize` package as
 * `packages/protocol`, for the same canonicalization guarantee. */
export function canonicalClaimSetJson(claimSet: ClaimSet): string {
  const json = canonicalize(claimSet);
  if (json === undefined) {
    throw new Error("ClaimSet contains a value canonicalize cannot serialize (e.g. undefined)");
  }
  return json;
}

export function claimSetDigest(claimSet: ClaimSet): string {
  return createHash("sha256").update(canonicalClaimSetJson(claimSet)).digest("hex");
}
