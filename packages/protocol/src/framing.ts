// Unambiguous domain-separated hashing: `H(tag, fields...)` from
// the protocol specification:
//
//   H(tag, fields...) = SHA256(u32be(len(tag)) || utf8(tag) || Σ[u64be(len(field)) || field])
//
// Length-prefixing every component (rather than, say, joining with a
// separator byte) is what makes the framing unambiguous: no sequence of
// tag/field byte lengths can be reinterpreted as a different tag/field
// split. Mirrors crates/commitments/src/framing.rs exactly.

import { createHash } from "node:crypto";

export class FramingError extends Error {}

const MAX_U32 = 0xffffffff;
// Node can address at most Number.MAX_SAFE_INTEGER bytes in a Buffer in
// practice (far below the true u64 range), so the field-length overflow
// guard below is unreachable on any real input; it exists so the encoder's
// contract matches the Rust side's, not because it is expected to fire.
const MAX_SAFE_FIELD_LEN = Number.MAX_SAFE_INTEGER;

/**
 * Computes `H(tag, fields...)`. Rejects (rather than silently truncating)
 * a tag or field whose byte length cannot be represented in the framing's
 * fixed-width length prefix.
 */
export function frameHash(tag: string, fields: Uint8Array[]): Uint8Array {
  const tagBytes = new TextEncoder().encode(tag);
  if (tagBytes.length > MAX_U32) {
    throw new FramingError(`tag length ${tagBytes.length} exceeds u32 range`);
  }

  const hash = createHash("sha256");
  hash.update(encodeU32BE(tagBytes.length));
  hash.update(tagBytes);
  for (const field of fields) {
    if (field.length > MAX_SAFE_FIELD_LEN) {
      throw new FramingError(`field length ${field.length} exceeds representable range`);
    }
    hash.update(encodeU64BE(field.length));
    hash.update(field);
  }
  return new Uint8Array(hash.digest());
}

function encodeU32BE(value: number): Uint8Array {
  const buf = new Uint8Array(4);
  new DataView(buf.buffer).setUint32(0, value, false);
  return buf;
}

function encodeU64BE(value: number): Uint8Array {
  const buf = new Uint8Array(8);
  new DataView(buf.buffer).setBigUint64(0, BigInt(value), false);
  return buf;
}
