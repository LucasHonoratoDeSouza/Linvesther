// Multi-account consolidation, per the protocol specification:
// "Em qualquer corte, incluir todas as contas membros... Mudança em
// qualquer conta afeta o cálculo agregado, nunca só um detalhe oculto
// de UI." Mirrors crates/domain/consolidation's rules — this is
// a from-scratch TypeScript reimplementation, not a shared import,
// since apps/api is its own runtime independent of the Rust domain
// crates (the same boundary every other apps/api module already
// crosses this way).

export interface MemberAccount {
  accountId: string;
  joinedAtMs: number;
  /** Already valued in the consolidated currency at this cut, fixed
   * integer micros (1 unit = 1e-6 of the consolidated currency). */
  navMicros: number;
  /** Whether this member has a coverage gap at this cut. */
  hasGap: boolean;
}

export interface TrackMembership {
  trackId: string;
  members: MemberAccount[];
}

export interface InternalTransfer {
  fromAccountId: string;
  toAccountId: string;
  outgoingMicros: number;
  feeMicros: number;
  incomingMicros: number;
}

export type AggregateResult =
  | { status: "available"; navMicros: number }
  | { status: "unavailable"; reason: "member_gap"; accountId: string };
