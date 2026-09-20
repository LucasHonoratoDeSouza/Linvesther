// CSPRNG salt generation for commitments.
//
// Per the protocol specification: "Salt vem de CSPRNG... nunca é
// derivado apenas de saldo, UID ou senha." This module only ever draws
// from the OS CSPRNG (via Node's `crypto.randomBytes`) — there is no
// constructor that derives a salt from caller-supplied data.

import { randomBytes } from "node:crypto";

/** Draws a fresh 256-bit salt from the OS CSPRNG. */
export function generateSalt(): Uint8Array {
  return new Uint8Array(randomBytes(32));
}
