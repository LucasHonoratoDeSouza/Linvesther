// Claims and disclosure, per the protocol specification: "Owner
// assina ClaimSet: identidade, track, checkpoint, intervalo, claims,
// audience, nonce e validade" and the protocol specification:
// "assinar digest dos claims, período, checkpoint e audience."

export type ClaimPredicate =
  | { type: "GE" | "GT" | "LE" | "LT"; metric: string; threshold: string }
  | { type: "RANGE"; metric: string; min: string; max: string }
  | { type: "VALUE"; metric: string; value: string };

export interface ClaimSet {
  identityId: string;
  trackId: string;
  /** Links this disclosure to the exact checkpoint its claims were
   * computed against — part of what the owner's signature covers, so
   * pointing the same claims at a different checkpoint after signing
   * invalidates consent. */
  checkpointId: string;
  periodStart: string;
  periodEnd: string;
  claims: ClaimPredicate[];
  /** `"public"` (reusable by anyone) or a specific requester identifier. */
  audience: string;
  nonce: string;
  expiresAt: string;
}

export type DisclosureError = "expired" | "audience_mismatch" | "signature_invalid";
