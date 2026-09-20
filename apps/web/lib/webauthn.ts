// Client-side WebAuthn ceremony: navigator.credentials.create/get produce
// ArrayBuffer-based PublicKeyCredential objects, while apps/api's
// @simplewebauthn/server expects the base64url JSON encoding of those
// same fields (RegistrationResponseJSON/AuthenticationResponseJSON).
// This module is the boundary that converts between the two — it never
// handles a private key, since WebAuthn never exposes one to script.

export class WebAuthnUnsupportedError extends Error {
  constructor() {
    super("This browser does not support WebAuthn passkeys.");
  }
}

export class WebAuthnCancelledError extends Error {
  constructor() {
    super("The passkey prompt was cancelled or timed out.");
  }
}

/** Must be checked before ever calling navigator.credentials.create/get
 * — a non-secure context or an old browser either lacks
 * `window.PublicKeyCredential` entirely or throws a generic error if
 * called anyway; detecting this upfront lets the caller offer the vault
 * method instead of attempting and failing. */
export function isWebAuthnSupported(): boolean {
  return typeof window !== "undefined" && window.PublicKeyCredential !== undefined;
}

function base64UrlEncode(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function base64UrlDecode(value: string): ArrayBuffer {
  const padded = value.replace(/-/g, "+").replace(/_/g, "/");
  const paddingNeeded = (4 - (padded.length % 4)) % 4;
  const binary = atob(padded + "=".repeat(paddingNeeded));
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}

export interface PublicKeyCredentialCreationOptionsJSON {
  rp: { name: string; id?: string };
  user: { id: string; name: string; displayName: string };
  challenge: string;
  pubKeyCredParams: { type: "public-key"; alg: number }[];
  timeout?: number;
  excludeCredentials?: { id: string; type: "public-key"; transports?: string[] }[];
  authenticatorSelection?: {
    authenticatorAttachment?: string;
    residentKey?: string;
    requireResidentKey?: boolean;
    userVerification?: string;
  };
  attestation?: string;
}

export interface PublicKeyCredentialRequestOptionsJSON {
  challenge: string;
  timeout?: number;
  rpId?: string;
  allowCredentials?: { id: string; type: "public-key"; transports?: string[] }[];
  userVerification?: string;
}

export interface RegistrationResponseJSON {
  id: string;
  rawId: string;
  response: { clientDataJSON: string; attestationObject: string };
  clientExtensionResults: Record<string, never>;
  type: "public-key";
}

export interface AuthenticationResponseJSON {
  id: string;
  rawId: string;
  response: { clientDataJSON: string; authenticatorData: string; signature: string; userHandle?: string };
  clientExtensionResults: Record<string, never>;
  type: "public-key";
}

function toCreationOptions(json: PublicKeyCredentialCreationOptionsJSON): PublicKeyCredentialCreationOptions {
  return {
    ...json,
    challenge: base64UrlDecode(json.challenge),
    user: { ...json.user, id: base64UrlDecode(json.user.id) },
    excludeCredentials: json.excludeCredentials?.map((credential) => ({
      ...credential,
      id: base64UrlDecode(credential.id),
    })) as PublicKeyCredentialDescriptor[] | undefined,
  } as PublicKeyCredentialCreationOptions;
}

function toRequestOptions(json: PublicKeyCredentialRequestOptionsJSON): PublicKeyCredentialRequestOptions {
  return {
    ...json,
    challenge: base64UrlDecode(json.challenge),
    allowCredentials: json.allowCredentials?.map((credential) => ({
      ...credential,
      id: base64UrlDecode(credential.id),
    })) as PublicKeyCredentialDescriptor[] | undefined,
  } as PublicKeyCredentialRequestOptions;
}

function asCancellation(cause: unknown): WebAuthnCancelledError | undefined {
  if (cause instanceof DOMException && cause.name === "NotAllowedError") {
    return new WebAuthnCancelledError();
  }
  return undefined;
}

/** Registers a new passkey from options the backend issued via
 * `/auth/webauthn/register-options`. Throws `WebAuthnUnsupportedError`
 * before ever calling `navigator.credentials.create` if the browser
 * lacks WebAuthn, and `WebAuthnCancelledError` if the user
 * cancels or the authenticator times out — in both failure
 * cases nothing is sent to the backend, so no partial registration
 * state is ever created. */
export async function registerPasskey(options: PublicKeyCredentialCreationOptionsJSON): Promise<RegistrationResponseJSON> {
  if (!isWebAuthnSupported()) {
    throw new WebAuthnUnsupportedError();
  }

  let credential: PublicKeyCredential;
  try {
    credential = (await navigator.credentials.create({ publicKey: toCreationOptions(options) })) as PublicKeyCredential;
  } catch (cause) {
    throw asCancellation(cause) ?? cause;
  }

  const response = credential.response as AuthenticatorAttestationResponse;
  return {
    id: credential.id,
    rawId: base64UrlEncode(credential.rawId),
    response: {
      clientDataJSON: base64UrlEncode(response.clientDataJSON),
      attestationObject: base64UrlEncode(response.attestationObject),
    },
    clientExtensionResults: {},
    type: "public-key",
  };
}

/** Authenticates with an existing passkey from options the backend
 * issued via `/auth/webauthn/login-options`. Same support-detection and
 * cancellation handling as `registerPasskey`. */
export async function authenticatePasskey(options: PublicKeyCredentialRequestOptionsJSON): Promise<AuthenticationResponseJSON> {
  if (!isWebAuthnSupported()) {
    throw new WebAuthnUnsupportedError();
  }

  let credential: PublicKeyCredential;
  try {
    credential = (await navigator.credentials.get({ publicKey: toRequestOptions(options) })) as PublicKeyCredential;
  } catch (cause) {
    throw asCancellation(cause) ?? cause;
  }

  const response = credential.response as AuthenticatorAssertionResponse;
  return {
    id: credential.id,
    rawId: base64UrlEncode(credential.rawId),
    response: {
      clientDataJSON: base64UrlEncode(response.clientDataJSON),
      authenticatorData: base64UrlEncode(response.authenticatorData),
      signature: base64UrlEncode(response.signature),
      userHandle: response.userHandle ? base64UrlEncode(response.userHandle) : undefined,
    },
    clientExtensionResults: {},
    type: "public-key",
  };
}
