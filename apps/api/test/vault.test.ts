import { describe, expect, it } from "vitest";
import { beginChallenge, verifyRawSignature, verifySignature } from "../src/auth/vault.js";

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
}

async function generateKeyPair() {
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  // raw is 0x04 || X(32) || Y(32) (uncompressed point)
  const qx = bytesToHex(raw.slice(1, 33));
  const qy = bytesToHex(raw.slice(33, 65));
  return { privateKey, qx, qy };
}

async function sign(privateKey: CryptoKey, message: string): Promise<`0x${string}`> {
  const signature = await crypto.subtle.sign(
    { name: "ECDSA", hash: "SHA-256" },
    privateKey,
    new TextEncoder().encode(message),
  );
  return bytesToHex(new Uint8Array(signature));
}

describe("vault verifySignature", () => {
  it("accepts a real P-256 signature over an issued challenge", async () => {
    const { privateKey, qx, qy } = await generateKeyPair();
    const challenge = await beginChallenge();
    const signature = await sign(privateKey, challenge);

    const result = await verifySignature(qx, qy, challenge, signature);
    expect(result).toBe(true);
  });

  it("rejects a signature produced by a different key", async () => {
    const owner = await generateKeyPair();
    const stranger = await generateKeyPair();
    const challenge = await beginChallenge();
    const signature = await sign(stranger.privateKey, challenge);

    const result = await verifySignature(owner.qx, owner.qy, challenge, signature);
    expect(result).toBe(false);
  });

  it("rejects a challenge that was never issued", async () => {
    const { privateKey, qx, qy } = await generateKeyPair();
    const signature = await sign(privateKey, "never-issued-challenge");

    const result = await verifySignature(qx, qy, "never-issued-challenge", signature);
    expect(result).toBe(false);
  });

  it("rejects a replayed challenge on the second verification attempt", async () => {
    const { privateKey, qx, qy } = await generateKeyPair();
    const challenge = await beginChallenge();
    const signature = await sign(privateKey, challenge);

    const first = await verifySignature(qx, qy, challenge, signature);
    expect(first).toBe(true);

    const replay = await verifySignature(qx, qy, challenge, signature);
    expect(replay).toBe(false);
  });
});

describe("vault verifyRawSignature", () => {
  it("accepts a real P-256 signature over an arbitrary message with no prior issuance", async () => {
    const { privateKey, qx, qy } = await generateKeyPair();
    const signature = await sign(privateKey, "arbitrary-application-digest");

    const result = await verifyRawSignature(qx, qy, "arbitrary-application-digest", signature);
    expect(result).toBe(true);
  });

  it("rejects a signature produced by a different key", async () => {
    const owner = await generateKeyPair();
    const stranger = await generateKeyPair();
    const signature = await sign(stranger.privateKey, "arbitrary-application-digest");

    const result = await verifyRawSignature(owner.qx, owner.qy, "arbitrary-application-digest", signature);
    expect(result).toBe(false);
  });

  it("can verify the same message more than once — no replay bookkeeping at this layer", async () => {
    const { privateKey, qx, qy } = await generateKeyPair();
    const signature = await sign(privateKey, "arbitrary-application-digest");

    const first = await verifyRawSignature(qx, qy, "arbitrary-application-digest", signature);
    const second = await verifyRawSignature(qx, qy, "arbitrary-application-digest", signature);
    expect(first).toBe(true);
    expect(second).toBe(true);
  });
});
