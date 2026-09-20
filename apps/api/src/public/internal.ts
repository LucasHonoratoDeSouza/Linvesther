import type { AvailabilityStatus, CalculationStatus, CoverageStatus, OriginBadge, PublicCorrection, PublicGap, RegistryStatus } from "./types.js";

/** The internal, private record a real deployment would read from
 * PostgreSQL — combining the checkpoint/coverage projection, the
 * corrections ledger and the indexer's registry status.
 * Deliberately carries the exact fields the protocol says a public profile
 * must never expose, so `toPublicProjection` (and the tests on it) have
 * something real to strip rather than an already-safe fixture. */
export interface InternalTrackRecord {
  identityId: string;
  trackId: string;
  createdAt: string;
  verifiedSince: string | null;
  currency: string;
  originMechanism: OriginBadge;
  coverageStatus: CoverageStatus;
  calculationStatus: CalculationStatus;
  registryStatus: RegistryStatus;
  availabilityStatus: AvailabilityStatus;
  gaps: PublicGap[];
  corrections: PublicCorrection[];

  // --- private fields the protocol forbids in any public profile ---
  exactBalanceUsd: string;
  operations: { symbol: string; side: "buy" | "sell"; quantity: string }[];
  sourceApiKeyDigest: string;
  institutionWalletId: string;
  witnessBlob: string;
  sourceNamespaceUid: string;
}
