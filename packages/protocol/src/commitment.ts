// dataCommitment = H("LZK/data/v1", salt32, JCS(data))

import { canonicalizeValue } from "./canonical.js";
import { frameHash } from "./framing.js";

export const DATA_COMMITMENT_TAG = "LZK/data/v1";

/**
 * Computes a private data commitment: changing the salt, or changing any
 * field of `data` (which changes its canonical encoding), changes the
 * commitment. `data` must already be validated by the caller (this
 * function canonicalizes a trusted value; use `canonicalize` from
 * `./canonical.js` first if `data` originates as raw, untrusted JSON text).
 */
export function dataCommitment(salt: Uint8Array, data: unknown): Uint8Array {
  const canonical = canonicalizeValue(data);
  return frameHash(DATA_COMMITMENT_TAG, [salt, canonical]);
}
