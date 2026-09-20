# collector-attestation

A0 origin attestation, per the protocol specification's
"SourceEnvelope v1" and "A0 é assinatura de coletor identificado"
sections.

## Scope

- `envelope::SourceEnvelope` carries the fields named in proofs.md:
  `mechanism`, `issuerKeyId`, `trustManifestHash`,
  `accountBindingCommitment`, `batchCommitment`, `rawEvidenceRoot`,
  `normalizedRoot`, `coverageManifestHash`, `period` (as start/end),
  `observedAt`, `expiresAt`, `policyHash`, `environment`,
  `sourceSessionBinding`. `digest()` hashes every field,
  length-prefixed and domain-separated so no two distinct envelopes ever
  collide, including across a shifted string boundary.
- `signer::A0Signer` signs an envelope's digest with secp256k1/ECDSA
  (`k256`), covering commitments, policy, binding and the interval in one
  signature. `verify` checks a signature against the exact envelope it
  claims to cover; changing any single field — even one byte of a
  32-byte commitment — invalidates it.
- `badge::origin_badge` derives the origin badge from the envelope's own
  declared `mechanism` alone. It takes no other input, so nothing else
  (a valid calculation proof, a clean reputation) can elevate an A0
  envelope's badge — proving the math correctly never changes what was
  proven about the origin.

## Out of scope

This crate does not implement A1 (TLS origin proof, notary/verifier) or
A2 (institution-issued signatures) attestation mechanisms, nor EIP-712
typed-data encoding for the eventual EVM binding — it delivers the A0
signer and the badge invariant only.
