// Merkle tree per the protocol specification:
//
// - leaf: H("LZK/leaf/v1", salt32, JCS(record))
// - node: H("LZK/node/v1", left32, right32), child order preserved
// - odd-sized level: duplicate the last node to pair it with itself
// - external root: H("LZK/tree/v1", u64be(count), top32)
// - empty tree: top32 = H("LZK/empty/v1"), count = 0
//
// Mirrors crates/commitments/src/merkle.rs.

import { canonicalizeValue } from "./canonical.js";
import { frameHash } from "./framing.js";

export const LEAF_TAG = "LZK/leaf/v1";
export const NODE_TAG = "LZK/node/v1";
export const TREE_TAG = "LZK/tree/v1";
export const EMPTY_TAG = "LZK/empty/v1";

export class MerkleError extends Error {}

/** Hashes one private leaf: H("LZK/leaf/v1", salt32, JCS(record)). */
export function leafHash(salt: Uint8Array, record: unknown): Uint8Array {
  return frameHash(LEAF_TAG, [salt, canonicalizeValue(record)]);
}

/**
 * Hashes one internal node: H("LZK/node/v1", left32, right32). Child order
 * is preserved by construction — this function never sorts its arguments,
 * so swapping left/right changes the result.
 */
export function nodeHash(left: Uint8Array, right: Uint8Array): Uint8Array {
  return frameHash(NODE_TAG, [left, right]);
}

function emptyTop(): Uint8Array {
  return frameHash(EMPTY_TAG, []);
}

function externalRoot(count: number, top: Uint8Array): Uint8Array {
  const countBytes = new Uint8Array(8);
  new DataView(countBytes.buffer).setBigUint64(0, BigInt(count), false);
  return frameHash(TREE_TAG, [countBytes, top]);
}

export interface InclusionProof {
  index: number;
  count: number;
  siblings: Uint8Array[];
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function reduceLevel(level: Uint8Array[]): Uint8Array[] {
  const next: Uint8Array[] = [];
  for (let i = 0; i < level.length; i += 2) {
    const left = level[i] as Uint8Array;
    const right = i + 1 < level.length ? (level[i + 1] as Uint8Array) : left; // duplicate last on odd count
    next.push(nodeHash(left, right));
  }
  return next;
}

function expectedHeight(count: number): number {
  if (count <= 1) return 0;
  let height = 0;
  let n = count;
  while (n > 1) {
    n = Math.ceil(n / 2);
    height += 1;
  }
  return height;
}

/** A built Merkle tree over already-hashed leaves. */
export class MerkleTree {
  readonly root: Uint8Array;
  readonly count: number;
  private readonly levels: Uint8Array[][];

  private constructor(root: Uint8Array, count: number, levels: Uint8Array[][]) {
    this.root = root;
    this.count = count;
    this.levels = levels;
  }

  /**
   * Builds a tree over `leaves` (already leaf-hashed; this class does not
   * hold the private salts/records needed to recompute leaf hashes).
   */
  static build(leaves: Uint8Array[]): MerkleTree {
    if (leaves.length === 0) {
      return new MerkleTree(externalRoot(0, emptyTop()), 0, []);
    }
    const count = leaves.length;
    const levels: Uint8Array[][] = [leaves];
    while ((levels[levels.length - 1] as Uint8Array[]).length > 1) {
      levels.push(reduceLevel(levels[levels.length - 1] as Uint8Array[]));
    }
    const top = (levels[levels.length - 1] as Uint8Array[])[0] as Uint8Array;
    return new MerkleTree(externalRoot(count, top), count, levels);
  }

  /**
   * The number of sibling hashes any valid proof over this tree must
   * carry — one per reduction level, 0 for a single-leaf tree.
   */
  height(): number {
    return Math.max(0, this.levels.length - 1);
  }

  prove(index: number): InclusionProof {
    if (this.count === 0) {
      throw new MerkleError("cannot prove inclusion in an empty tree");
    }
    if (index >= this.count) {
      throw new MerkleError(`index ${index} out of range for count ${this.count}`);
    }
    const siblings: Uint8Array[] = [];
    let i = index;
    for (let level = 0; level < this.levels.length - 1; level++) {
      const currentLevel = this.levels[level] as Uint8Array[];
      const siblingIndex = i % 2 === 0 ? (i + 1 < currentLevel.length ? i + 1 : i) : i - 1;
      siblings.push(currentLevel[siblingIndex] as Uint8Array);
      i = Math.floor(i / 2);
    }
    return { index, count: this.count, siblings };
  }
}

/**
 * Verifies that `leaf` is included at `proof.index` of a tree of
 * `proof.count` leaves whose external root is `expectedRoot`. Rejects an
 * index outside `count`, a sibling list of the wrong length for the tree's
 * height (whether short or padded with excess entries), and any
 * recomputed root that does not match.
 */
export function verifyInclusion(leaf: Uint8Array, proof: InclusionProof, expectedRoot: Uint8Array): void {
  if (proof.count === 0) {
    throw new MerkleError("cannot prove inclusion in an empty tree");
  }
  if (proof.index >= proof.count) {
    throw new MerkleError(`index ${proof.index} out of range for count ${proof.count}`);
  }
  const height = expectedHeight(proof.count);
  if (proof.siblings.length !== height) {
    throw new MerkleError(
      `proof carries ${proof.siblings.length} siblings, expected exactly ${height} for this tree height`,
    );
  }

  let hash = leaf;
  let index = proof.index;
  for (const sibling of proof.siblings) {
    hash = index % 2 === 0 ? nodeHash(hash, sibling) : nodeHash(sibling, hash);
    index = Math.floor(index / 2);
  }
  const recomputed = externalRoot(proof.count, hash);
  if (!bytesEqual(recomputed, expectedRoot)) {
    throw new MerkleError("recomputed root does not match the expected root");
  }
}
