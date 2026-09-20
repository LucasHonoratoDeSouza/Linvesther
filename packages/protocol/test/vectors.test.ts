// Cross-language conformance: reproduces every value in
// fixtures/commitments/vectors.json, the same fixture the Rust suite in
// crates/commitments checks against. A mismatch here means the two
// languages would produce different commitments for identical inputs.

import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  canonicalizeValue,
  frameHash,
  dataCommitment,
  leafHash,
  MerkleTree,
  verifyInclusion,
} from "../src/index.js";

const fixturePath = fileURLToPath(
  new URL("../../../fixtures/commitments/vectors.json", import.meta.url),
);
const vectors = JSON.parse(readFileSync(fixturePath, "utf8"));

function hex32(s: string): Uint8Array {
  const bytes = new Uint8Array(s.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(s.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

describe("canonical encoding", () => {
  it("matches the fixture", () => {
    const actual = canonicalizeValue(vectors.canonical.input);
    expect(new TextDecoder().decode(actual)).toBe(vectors.canonical.expectedBytesUtf8);
  });
});

describe("frame hash", () => {
  it("matches the fixture", () => {
    const fields: Uint8Array[] = vectors.frameHash.fieldsUtf8.map(
      (f: string) => new TextEncoder().encode(f),
    );
    const actual = frameHash(vectors.frameHash.tag, fields);
    expect(toHex(actual)).toBe(vectors.frameHash.expectedHex);
  });
});

describe("data commitment", () => {
  it("matches the fixture and reacts to salt/field changes", () => {
    const data = vectors.dataCommitment.data;
    const commitments: Uint8Array[] = [];
    for (const c of vectors.dataCommitment.cases) {
      const salt = hex32(c.saltHex);
      const actual = dataCommitment(salt, data);
      expect(toHex(actual), `case ${c.name}`).toBe(c.expectedHex);
      commitments.push(actual);
    }
    expect(toHex(commitments[0]!)).not.toBe(toHex(commitments[1]!));

    const fieldChanged = vectors.dataCommitment.fieldChangedCase;
    const actual = dataCommitment(hex32(fieldChanged.saltHex), fieldChanged.data);
    expect(toHex(actual)).toBe(fieldChanged.expectedHex);
    expect(toHex(actual)).not.toBe(toHex(commitments[0]!));
  });
});

describe("merkle tree", () => {
  it("matches the fixture end to end", () => {
    const m = vectors.merkle;
    const leaves: Uint8Array[] = m.leafRecords.map((record: unknown, i: number) =>
      leafHash(hex32(m.leafSaltsHex[i]), record),
    );
    const expectedLeafHashes: string[] = m.expectedLeafHashesHex;
    expect(leaves.map(toHex)).toEqual(expectedLeafHashes);

    const tree = MerkleTree.build(leaves);
    expect(toHex(tree.root)).toBe(m.expectedRootHex);
    expect(tree.height()).toBe(m.expectedHeight);

    const proofFixture = m.inclusionProof;
    const proof = tree.prove(proofFixture.index);
    expect(proof.siblings.map(toHex)).toEqual(proofFixture.expectedSiblingsHex);
    expect(proof.count).toBe(m.leafRecords.length);

    expect(() => verifyInclusion(leaves[proofFixture.index]!, proof, tree.root)).not.toThrow();

    const emptyTree = MerkleTree.build([]);
    expect(toHex(emptyTree.root)).toBe(m.emptyTree.expectedRootHex);

    const singleTree = MerkleTree.build([leaves[0]!]);
    expect(toHex(singleTree.root)).toBe(m.singleLeafTree.expectedRootHex);
    expect(singleTree.height()).toBe(m.singleLeafTree.expectedHeight);
  });
});
