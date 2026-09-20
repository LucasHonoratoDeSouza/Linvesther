import type { InternalTrackRecord } from "./internal.js";
import type { TrackProjection } from "./types.js";

/** Builds the public projection by naming exactly the fields that are
 * safe, rather than by deleting the unsafe ones from a copy — so a new
 * private field added to `InternalTrackRecord` later can never leak
 * silently through this function; it would have to be added here on
 * purpose. */
export function toPublicProjection(record: InternalTrackRecord): TrackProjection {
  return {
    profile: {
      identityId: record.identityId,
      trackId: record.trackId,
      createdAt: record.createdAt,
      verifiedSince: record.verifiedSince,
      currency: record.currency,
    },
    dimensions: {
      origin: record.originMechanism,
      coverage: record.coverageStatus,
      calculation: record.calculationStatus,
      registry: record.registryStatus,
      availability: record.availabilityStatus,
    },
    gaps: record.gaps,
    corrections: record.corrections,
  };
}
