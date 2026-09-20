// Adversarial cases the conformance gate requires: salt/field changes are caught
// (also exercised with fixture vectors in vectors.test.ts), duplicate keys
// and float-syntax numbers are rejected, and Merkle inclusion proofs with
// invalid padding/index/height are rejected.

import { describe, expect, it } from "vitest";
import {
  canonicalize,
  CanonicalError,
  frameHash,
  leafHash,
  nodeHash,
  MerkleTree,
  MerkleError,
  verifyInclusion,
} from "../src/index.js";

describe("strict canonicalization", () => {
  it("rejects a duplicate top-level key", () => {
    expect(() => canonicalize('{"a":1,"a":2}')).toThrow(CanonicalError);
  });

  it("rejects a duplicate nested key", () => {
    expect(() => canonicalize('{"outer":{"x":1,"x":2}}')).toThrow(CanonicalError);
  });

  it("rejects a duplicate key inside an array element", () => {
    expect(() => canonicalize('{"list":[{"x":1,"x":2}]}')).toThrow(CanonicalError);
  });

  it("rejects a float-syntax number", () => {
    expect(() => canonicalize('{"amount":1.5}')).toThrow(CanonicalError);
  });

  it("rejects an exponent-syntax number", () => {
    expect(() => canonicalize('{"amount":1e10}')).toThrow(CanonicalError);
  });

  it("rejects NaN/Infinity (not valid JSON syntax)", () => {
    expect(() => canonicalize('{"amount":NaN}')).toThrow(CanonicalError);
    expect(() => canonicalize('{"amount":Infinity}')).toThrow(CanonicalError);
  });

  it("accepts plain integers", () => {
    expect(() => canonicalize('{"count":42,"negative":-7,"zero":0}')).not.toThrow();
  });
});

describe("leaf/node hashing", () => {
  it("changes with the salt", () => {
    const record = { x: 1 };
    const a = leafHash(new Uint8Array(32).fill(1), record);
    const b = leafHash(new Uint8Array(32).fill(2), record);
    expect(a).not.toEqual(b);
  });

  it("changes with the field", () => {
    const salt = new Uint8Array(32).fill(1);
    const a = leafHash(salt, { x: 1 });
    const b = leafHash(salt, { x: 2 });
    expect(a).not.toEqual(b);
  });

  it("node hash is order sensitive", () => {
    const a = new Uint8Array(32).fill(1);
    const b = new Uint8Array(32).fill(2);
    expect(nodeHash(a, b)).not.toEqual(nodeHash(b, a));
  });
});

function sampleTree() {
  const leaves = Array.from({ length: 5 }, (_, i) => new Uint8Array(32).fill(i));
  return { tree: MerkleTree.build(leaves), leaves };
}

describe("merkle proof validation", () => {
  it("rejects an out-of-range prove() index", () => {
    const { tree } = sampleTree();
    expect(() => tree.prove(tree.count)).toThrow(MerkleError);
  });

  it("rejects verification with an out-of-range index", () => {
    const { tree, leaves } = sampleTree();
    const proof = tree.prove(0);
    proof.index = proof.count;
    expect(() => verifyInclusion(leaves[0]!, proof, tree.root)).toThrow(MerkleError);
  });

  it("rejects too few siblings (short padding)", () => {
    const { tree, leaves } = sampleTree();
    const proof = tree.prove(2);
    proof.siblings.pop();
    expect(() => verifyInclusion(leaves[2]!, proof, tree.root)).toThrow(MerkleError);
  });

  it("rejects excess siblings", () => {
    const { tree, leaves } = sampleTree();
    const proof = tree.prove(2);
    proof.siblings.push(new Uint8Array(32).fill(0xaa));
    expect(() => verifyInclusion(leaves[2]!, proof, tree.root)).toThrow(MerkleError);
  });

  it("rejects a tampered sibling value", () => {
    const { tree, leaves } = sampleTree();
    const proof = tree.prove(2);
    proof.siblings[0] = new Uint8Array(32).fill(0xff);
    expect(() => verifyInclusion(leaves[2]!, proof, tree.root)).toThrow(MerkleError);
  });

  it("rejects a proof claiming a different tree count", () => {
    const { tree, leaves } = sampleTree();
    const proof = tree.prove(2);
    proof.count = 6;
    expect(() => verifyInclusion(leaves[2]!, proof, tree.root)).toThrow(MerkleError);
  });

  it("has no valid proof for an empty tree", () => {
    const empty = MerkleTree.build([]);
    expect(() => empty.prove(0)).toThrow(MerkleError);
  });

  it("rejects a tampered leaf even with a correctly shaped proof", () => {
    const { tree } = sampleTree();
    const proof = tree.prove(2);
    const wrongLeaf = new Uint8Array(32).fill(0xee);
    expect(() => verifyInclusion(wrongLeaf, proof, tree.root)).toThrow(MerkleError);
  });
});

describe("framing", () => {
  it("succeeds for ordinary input", () => {
    expect(() => frameHash("tag", [new TextEncoder().encode("field")])).not.toThrow();
  });
});
