// Public projection, per the protocol specification's five
// independent dimensions (`Origem`/`Cobertura`/`Cálculo`/`Registro`/
// `Disponibilidade`) and the protocol specification:
// each dimension must be shown separately, and a public profile must
// never expose exact balance, operations, API keys, institution wallet
// IDs or witness data.

export type OriginBadge = "A0" | "A1" | "A2";
export type CoverageStatus = "POLICY_COMPLETE" | "INCOMPLETE" | "UNAVAILABLE";
export type CalculationStatus = "UNPROVEN" | "ZK_VERIFIED" | "INVALID";
export type RegistryStatus = "UNANCHORED" | "INCLUDED" | "SAFE" | "FINALIZED" | "REORGED";
export type AvailabilityStatus = "AVAILABLE" | "PARTIAL" | "UNAVAILABLE";

export interface PublicGap {
  startMs: number;
  endMs: number;
}

export interface PublicCorrection {
  targetDigest: string;
  reasonCode: string;
  effectiveFrom: string;
}

/** Always present, even when every dimension below is `UNAVAILABLE` —
 * per the acceptance criteria, the minimum profile stays visible regardless. */
export interface MinimalProfile {
  identityId: string;
  trackId: string;
  createdAt: string;
  /** `null` until a valid baseline exists — never backdated. */
  verifiedSince: string | null;
  /** The track's declared denomination (`TrackRecord`'s "denominação").
   * The v0.1 reference profile is always `"USDT"`. */
  currency: string;
}

export interface TrackProjection {
  profile: MinimalProfile;
  dimensions: {
    origin: OriginBadge;
    coverage: CoverageStatus;
    calculation: CalculationStatus;
    registry: RegistryStatus;
    availability: AvailabilityStatus;
  };
  gaps: PublicGap[];
  corrections: PublicCorrection[];
}
