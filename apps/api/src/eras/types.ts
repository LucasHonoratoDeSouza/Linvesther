// Declared strategy eras, per the protocol specification:
// "StrategyEra | marcador temporal e digest da descrição; metadado
// declarado, não prova de continuidade do algoritmo" and: a new era preserves lifetime and the declared label; a cutout
// never replaces the entire history.

export interface StrategyEra {
  eraId: string;
  /** Free-text label the owner declares — never verified as true, only
   * recorded as claimed. */
  label: string;
  /** sha256 hex of the era's description text, so the exact wording a
   * disclosure references can be checked later without republishing it
   * verbatim every time. */
  descriptionDigest: string;
  startMs: number;
  declaredBy: `0x${string}`;
}

/** The track's full lifetime plus every declared era layered on top of
 * it. `lifetimeStartMs` is the track's actual genesis and is never
 * moved or hidden by adding an era. */
export interface EraTimeline {
  lifetimeStartMs: number;
  eras: StrategyEra[];
}

/** A view scoped to one era. `isExcerpt` is always `true` here and
 * `lifetimeStartMs` always accompanies it, so a caller can never
 * present this as if it were the full lifetime — per proofs.md's
 * "Nunca rotular recorte favorável como lifetime."
 */
export interface EraScopedView {
  era: StrategyEra;
  isExcerpt: true;
  lifetimeStartMs: number;
}
