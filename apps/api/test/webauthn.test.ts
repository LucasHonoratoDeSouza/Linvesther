import type { AuthenticationResponseJSON, RegistrationResponseJSON } from "@simplewebauthn/server";
import { isoBase64URL, isoCBOR } from "@simplewebauthn/server/helpers";
import { describe, expect, it } from "vitest";
import { beginAuthentication, beginRegistration, finishAuthentication, finishRegistration } from "../src/auth/webauthn.js";

const CONFIG = { rpName: "Test RP", rpID: "localhost", origin: "https://localhost" };

// --- real P-256 key + WebAuthn wire-format helpers ---
// These mirror exactly what a browser authenticator produces (CBOR
// attestation object / COSE public key, DER-wrapped ECDSA signature),
// built with @simplewebauthn/server's own iso helpers plus real
// WebCrypto P-256 keys so the ceremony is exercised against genuine
// cryptography, not fabricated bytes.

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

async function generateKeyPair() {
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  const qx = raw.slice(1, 33);
  const qy = raw.slice(33, 65);
  return { privateKey, qx, qy };
}

function concatBytes(...parts: Uint8Array<ArrayBuffer>[]): Uint8Array<ArrayBuffer> {
  const total = parts.reduce((sum, p) => sum + p.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function encodeCosePublicKey(qx: Uint8Array<ArrayBuffer>, qy: Uint8Array<ArrayBuffer>): Uint8Array<ArrayBuffer> {
  const coseKey = new Map<number, number | Uint8Array<ArrayBuffer>>([
    [1, 2], // kty: EC2
    [3, -7], // alg: ES256
    [-1, 1], // crv: P-256
    [-2, qx],
    [-3, qy],
  ]);
  return isoCBOR.encode(coseKey) as Uint8Array<ArrayBuffer>;
}

async function buildAuthenticatorData(opts: {
  flags: number;
  counter: number;
  credentialId?: Uint8Array<ArrayBuffer>;
  cosePublicKey?: Uint8Array<ArrayBuffer>;
}): Promise<Uint8Array<ArrayBuffer>> {
  const rpIdHash = new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(CONFIG.rpID)));
  const flagsByte = new Uint8Array([opts.flags]);
  const counterBytes = new Uint8Array(4);
  new DataView(counterBytes.buffer).setUint32(0, opts.counter, false);

  if (opts.credentialId && opts.cosePublicKey) {
    const aaguid = new Uint8Array(16);
    const credIdLen = new Uint8Array(2);
    new DataView(credIdLen.buffer).setUint16(0, opts.credentialId.length, false);
    return concatBytes(rpIdHash, flagsByte, counterBytes, aaguid, credIdLen, opts.credentialId, opts.cosePublicKey);
  }
  return concatBytes(rpIdHash, flagsByte, counterBytes);
}

function buildClientDataJSON(
  type: "webauthn.create" | "webauthn.get",
  challenge: string,
  origin: string,
): Uint8Array<ArrayBuffer> {
  const json = JSON.stringify({ type, challenge, origin });
  return new TextEncoder().encode(json);
}

function derEncodeInteger(bytes: Uint8Array<ArrayBuffer>): Uint8Array<ArrayBuffer> {
  let start = 0;
  while (start < bytes.length - 1 && bytes[start] === 0 && (bytes[start + 1] ?? 0) < 0x80) {
    start++;
  }
  let trimmed = bytes.slice(start);
  if ((trimmed[0] ?? 0) & 0x80) {
    trimmed = concatBytes(new Uint8Array([0]), trimmed);
  }
  return concatBytes(new Uint8Array([0x02, trimmed.length]), trimmed);
}

function derEncodeSignature(rawSignature: Uint8Array<ArrayBuffer>): Uint8Array<ArrayBuffer> {
  const r = rawSignature.slice(0, 32);
  const s = rawSignature.slice(32, 64);
  const rDer = derEncodeInteger(r);
  const sDer = derEncodeInteger(s);
  const body = concatBytes(rDer, sDer);
  return concatBytes(new Uint8Array([0x30, body.length]), body);
}

async function buildRegistrationResponse(
  privateKeyPair: { qx: Uint8Array<ArrayBuffer>; qy: Uint8Array<ArrayBuffer> },
  challenge: string,
  credentialId: Uint8Array<ArrayBuffer>,
): Promise<RegistrationResponseJSON> {
  const cosePublicKey = encodeCosePublicKey(privateKeyPair.qx, privateKeyPair.qy);
  const authenticatorData = await buildAuthenticatorData({
    flags: 0x45, // UP | UV | AT
    counter: 0,
    credentialId,
    cosePublicKey,
  });
  const clientDataJSON = buildClientDataJSON("webauthn.create", challenge, CONFIG.origin);
  const attestationObject = isoCBOR.encode(
    new Map<string, string | Map<never, never> | Uint8Array<ArrayBuffer>>([
      ["fmt", "none"],
      ["attStmt", new Map<never, never>()],
      ["authData", authenticatorData],
    ]),
  );

  return {
    id: isoBase64URL.fromBuffer(credentialId),
    rawId: isoBase64URL.fromBuffer(credentialId),
    response: {
      clientDataJSON: isoBase64URL.fromBuffer(clientDataJSON),
      attestationObject: isoBase64URL.fromBuffer(attestationObject),
    },
    clientExtensionResults: {},
    type: "public-key",
  };
}

async function buildAuthenticationResponse(
  signingKey: CryptoKey,
  challenge: string,
  credentialId: Uint8Array<ArrayBuffer>,
): Promise<AuthenticationResponseJSON> {
  const authenticatorData = await buildAuthenticatorData({ flags: 0x05, counter: 0 }); // UP | UV
  const clientDataJSON = buildClientDataJSON("webauthn.get", challenge, CONFIG.origin);
  const clientDataHash = new Uint8Array(await crypto.subtle.digest("SHA-256", clientDataJSON));
  const signedData = concatBytes(authenticatorData, clientDataHash);
  const rawSignature = new Uint8Array(
    await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, signingKey, signedData),
  );
  const signature = derEncodeSignature(rawSignature);

  return {
    id: isoBase64URL.fromBuffer(credentialId),
    rawId: isoBase64URL.fromBuffer(credentialId),
    response: {
      clientDataJSON: isoBase64URL.fromBuffer(clientDataJSON),
      authenticatorData: isoBase64URL.fromBuffer(authenticatorData),
      signature: isoBase64URL.fromBuffer(signature),
    },
    clientExtensionResults: {},
    type: "public-key",
  };
}

// --- tests ---

describe("webauthn registration", () => {
  it("accepts a valid attestation built with a real P-256 key", async () => {
    const keyPair = await generateKeyPair();
    const credentialId = new Uint8Array([1, 2, 3, 4]);
    const options = await beginRegistration("alice", CONFIG);
    const response = await buildRegistrationResponse(keyPair, options.challenge, credentialId);

    const result = await finishRegistration(response, CONFIG);

    expect(result.verified).toBe(true);
    expect(result.qx).toBe(`0x${bytesToHex(keyPair.qx)}`);
    expect(result.qy).toBe(`0x${bytesToHex(keyPair.qy)}`);
    expect(result.credentialId).toBe(isoBase64URL.fromBuffer(credentialId));
  });

  it("rejects a replayed registration challenge on the second verification", async () => {
    const keyPair = await generateKeyPair();
    const credentialId = new Uint8Array([1, 2, 3, 4]);
    const options = await beginRegistration("alice", CONFIG);
    const response = await buildRegistrationResponse(keyPair, options.challenge, credentialId);

    const first = await finishRegistration(response, CONFIG);
    expect(first.verified).toBe(true);

    const replay = await finishRegistration(response, CONFIG);
    expect(replay.verified).toBe(false);
  });

  it("does not constrain registration to a specific authenticator attachment", async () => {
    const options = await beginRegistration("alice", CONFIG);
    expect(options.authenticatorSelection?.authenticatorAttachment).toBeUndefined();
  });
});

describe("webauthn authentication", () => {
  async function registerCredential() {
    const keyPair = await generateKeyPair();
    const credentialId = new Uint8Array([9, 9, 9, 9]);
    const regOptions = await beginRegistration("alice", CONFIG);
    const regResponse = await buildRegistrationResponse(keyPair, regOptions.challenge, credentialId);
    await finishRegistration(regResponse, CONFIG);
    return { keyPair, credentialId };
  }

  it("accepts a valid assertion signed by the registered key", async () => {
    const { keyPair, credentialId } = await registerCredential();
    const authOptions = await beginAuthentication(CONFIG);
    const authResponse = await buildAuthenticationResponse(keyPair.privateKey, authOptions.challenge, credentialId);

    const result = await finishAuthentication(authResponse, CONFIG);

    expect(result.verified).toBe(true);
    expect(result.qx).toBe(`0x${bytesToHex(keyPair.qx)}`);
    expect(result.qy).toBe(`0x${bytesToHex(keyPair.qy)}`);
  });

  it("rejects an assertion signed by a different key", async () => {
    const { credentialId } = await registerCredential();
    const stranger = await generateKeyPair();
    const authOptions = await beginAuthentication(CONFIG);
    const authResponse = await buildAuthenticationResponse(stranger.privateKey, authOptions.challenge, credentialId);

    const result = await finishAuthentication(authResponse, CONFIG);
    expect(result.verified).toBe(false);
  });

  it("rejects a replayed authentication challenge on the second verification", async () => {
    const { keyPair, credentialId } = await registerCredential();
    const authOptions = await beginAuthentication(CONFIG);
    const authResponse = await buildAuthenticationResponse(keyPair.privateKey, authOptions.challenge, credentialId);

    const first = await finishAuthentication(authResponse, CONFIG);
    expect(first.verified).toBe(true);

    const replay = await finishAuthentication(authResponse, CONFIG);
    expect(replay.verified).toBe(false);
  });

  it("rejects an assertion for a credential ID that was never registered", async () => {
    const stranger = await generateKeyPair();
    const authOptions = await beginAuthentication(CONFIG);
    const authResponse = await buildAuthenticationResponse(
      stranger.privateKey,
      authOptions.challenge,
      new Uint8Array([0xff, 0xff]),
    );

    const result = await finishAuthentication(authResponse, CONFIG);
    expect(result.verified).toBe(false);
  });
});
