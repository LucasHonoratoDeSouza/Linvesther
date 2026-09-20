import type { AuthenticationResponseJSON, RegistrationResponseJSON } from "@simplewebauthn/server";
import { isoBase64URL, isoCBOR } from "@simplewebauthn/server/helpers";
import { describe, expect, it } from "vitest";
import { beginChallenge } from "../src/auth/vault.js";
import { credentialStore, verify } from "../src/auth/identitySignature.js";
import { beginAuthentication, beginRegistration, finishRegistration } from "../src/auth/webauthn.js";

const WEBAUTHN_CONFIG = { rpName: "Test RP", rpID: "localhost", origin: "https://localhost" };

function bytesToHex(bytes: Uint8Array): `0x${string}` {
  return `0x${Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("")}`;
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

async function generateKeyPair() {
  const { publicKey, privateKey } = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = new Uint8Array(await crypto.subtle.exportKey("raw", publicKey));
  return { privateKey, qx: raw.slice(1, 33), qy: raw.slice(33, 65) };
}

// --- minimal WebAuthn wire-format builders (same technique as
// webauthn.test.ts: real CBOR/COSE bytes over a real P-256 key, only what
// this dispatch test needs — see that file for the fuller ceremony
// coverage of webauthn.ts itself). ---

function encodeCosePublicKey(qx: Uint8Array<ArrayBuffer>, qy: Uint8Array<ArrayBuffer>): Uint8Array<ArrayBuffer> {
  const coseKey = new Map<number, number | Uint8Array<ArrayBuffer>>([
    [1, 2],
    [3, -7],
    [-1, 1],
    [-2, qx],
    [-3, qy],
  ]);
  return isoCBOR.encode(coseKey) as Uint8Array<ArrayBuffer>;
}

async function buildAuthenticatorData(opts: {
  flags: number;
  credentialId?: Uint8Array<ArrayBuffer>;
  cosePublicKey?: Uint8Array<ArrayBuffer>;
}): Promise<Uint8Array<ArrayBuffer>> {
  const rpIdHash = new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(WEBAUTHN_CONFIG.rpID)),
  );
  const flagsByte = new Uint8Array([opts.flags]);
  const counterBytes = new Uint8Array(4);
  if (opts.credentialId && opts.cosePublicKey) {
    const aaguid = new Uint8Array(16);
    const credIdLen = new Uint8Array(2);
    new DataView(credIdLen.buffer).setUint16(0, opts.credentialId.length, false);
    return concatBytes(rpIdHash, flagsByte, counterBytes, aaguid, credIdLen, opts.credentialId, opts.cosePublicKey);
  }
  return concatBytes(rpIdHash, flagsByte, counterBytes);
}

function buildClientDataJSON(type: "webauthn.create" | "webauthn.get", challenge: string): Uint8Array<ArrayBuffer> {
  return new TextEncoder().encode(JSON.stringify({ type, challenge, origin: WEBAUTHN_CONFIG.origin }));
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
  const rDer = derEncodeInteger(rawSignature.slice(0, 32));
  const sDer = derEncodeInteger(rawSignature.slice(32, 64));
  const body = concatBytes(rDer, sDer);
  return concatBytes(new Uint8Array([0x30, body.length]), body);
}

async function buildRegistrationResponse(
  keyPair: { qx: Uint8Array<ArrayBuffer>; qy: Uint8Array<ArrayBuffer> },
  challenge: string,
  credentialId: Uint8Array<ArrayBuffer>,
): Promise<RegistrationResponseJSON> {
  const cosePublicKey = encodeCosePublicKey(keyPair.qx, keyPair.qy);
  const authenticatorData = await buildAuthenticatorData({ flags: 0x45, credentialId, cosePublicKey });
  const clientDataJSON = buildClientDataJSON("webauthn.create", challenge);
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
  const authenticatorData = await buildAuthenticatorData({ flags: 0x05 });
  const clientDataJSON = buildClientDataJSON("webauthn.get", challenge);
  const clientDataHash = new Uint8Array(await crypto.subtle.digest("SHA-256", clientDataJSON));
  const signedData = concatBytes(authenticatorData, clientDataHash);
  const rawSignature = new Uint8Array(
    await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, signingKey, signedData),
  );
  return {
    id: isoBase64URL.fromBuffer(credentialId),
    rawId: isoBase64URL.fromBuffer(credentialId),
    response: {
      clientDataJSON: isoBase64URL.fromBuffer(clientDataJSON),
      authenticatorData: isoBase64URL.fromBuffer(authenticatorData),
      signature: isoBase64URL.fromBuffer(derEncodeSignature(rawSignature)),
    },
    clientExtensionResults: {},
    type: "public-key",
  };
}

describe("identitySignature.verify", () => {
  it("dispatches to vault verification for a vault-registered subjectKey", async () => {
    const subjectKey = "0x1111111111111111111111111111111111111111" as const;
    const keyPair = await generateKeyPair();
    credentialStore.register(subjectKey, bytesToHex(keyPair.qx), bytesToHex(keyPair.qy), "vault");

    const challenge = await beginChallenge();
    const rawSignature = new Uint8Array(
      await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, keyPair.privateKey, new TextEncoder().encode(challenge)),
    );

    const result = await verify(subjectKey, challenge, { method: "vault", signature: bytesToHex(rawSignature) });
    expect(result).toBe(true);
  });

  it("dispatches to WebAuthn verification for a webauthn-registered subjectKey", async () => {
    const subjectKey = "0x2222222222222222222222222222222222222222" as const;
    const keyPair = await generateKeyPair();
    const credentialId = new Uint8Array([7, 7, 7, 7]);

    const regOptions = await beginRegistration("alice", WEBAUTHN_CONFIG);
    const regResponse = await buildRegistrationResponse(keyPair, regOptions.challenge, credentialId);
    const registration = await finishRegistration(regResponse, WEBAUTHN_CONFIG);
    if (!registration.qx || !registration.qy) {
      throw new Error("test setup failed: registration did not verify");
    }
    credentialStore.register(subjectKey, registration.qx, registration.qy, "webauthn");

    const authOptions = await beginAuthentication(WEBAUTHN_CONFIG);
    const authResponse = await buildAuthenticationResponse(keyPair.privateKey, authOptions.challenge, credentialId);

    const result = await verify(subjectKey, "unused-for-webauthn", { method: "webauthn", response: authResponse });
    expect(result).toBe(true);
  });

  it("returns false for a subjectKey that was never registered", async () => {
    const result = await verify("0x9999999999999999999999999999999999999999", "any-digest", {
      method: "vault",
      signature: "0x00",
    });
    expect(result).toBe(false);
  });
});
