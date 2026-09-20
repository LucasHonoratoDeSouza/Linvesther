import type { FastifyInstance } from "fastify";
import { requireSession } from "../auth/requireSession.js";
import type { SessionStore } from "../auth/sessionStore.js";
import { consolidatedNav, TransferReconciliationError } from "./aggregate.js";
import type { MemoryMultiAccountStore } from "./store.js";

export interface MultiAccountRouteOptions {
  sessionStore: SessionStore;
  multiAccountStore: MemoryMultiAccountStore;
  /** trackId -> the only address allowed to manage its membership. */
  trackOwners: Map<string, `0x${string}`>;
  now: () => Date;
}

/** Registers `apps/api/multi-account`. Membership itself is
 * public (anyone can see which accounts feed a track's aggregate,
 * per "membership público"); only the track's owner can add a member
 * or record a transfer between members. */
export function registerMultiAccountRoutes(app: FastifyInstance, options: MultiAccountRouteOptions): void {
  app.post<{ Params: { trackId: string }; Body: { accountId: string; navMicros: number } }>(
    "/tracks/:trackId/members",
    async (request, reply) => {
      const session = await requireSession(request, options.sessionStore);
      if (!session) {
        return reply.code(401).send({ error: "unauthenticated" });
      }
      const owner = options.trackOwners.get(request.params.trackId);
      if (!owner || owner.toLowerCase() !== session.address.toLowerCase()) {
        return reply.code(403).send({ error: "cross_account_access_denied" });
      }
      const membership = options.multiAccountStore.addMember(request.params.trackId, {
        accountId: request.body.accountId,
        joinedAtMs: options.now().getTime(),
        navMicros: request.body.navMicros,
        hasGap: false,
      });
      return reply.code(201).send(membership);
    },
  );

  app.get<{ Params: { trackId: string } }>("/tracks/:trackId/members", async (request, reply) => {
    const membership = options.multiAccountStore.getMembership(request.params.trackId);
    if (!membership) {
      return reply.code(404).send({ error: "not_found" });
    }
    return membership;
  });

  app.post<{
    Params: { trackId: string };
    Body: { fromAccountId: string; toAccountId: string; outgoingMicros: number; feeMicros: number; incomingMicros: number };
  }>("/tracks/:trackId/transfers", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    const owner = options.trackOwners.get(request.params.trackId);
    if (!owner || owner.toLowerCase() !== session.address.toLowerCase()) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    try {
      const membership = options.multiAccountStore.recordTransfer(request.params.trackId, request.body);
      return reply.code(201).send(membership);
    } catch (error) {
      if (error instanceof TransferReconciliationError) {
        return reply.code(400).send({ error: "transfer_unreconciled" });
      }
      throw error;
    }
  });

  // Public: the consolidated NAV, or an explicit refusal (never a
  // partial sum) when any member has a coverage gap.
  app.get<{ Params: { trackId: string } }>("/tracks/:trackId/aggregate", async (request, reply) => {
    const membership = options.multiAccountStore.getMembership(request.params.trackId);
    if (!membership) {
      return reply.code(404).send({ error: "not_found" });
    }
    return consolidatedNav(membership.members);
  });
}
