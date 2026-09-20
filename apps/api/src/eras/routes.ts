import { createHash } from "node:crypto";
import type { FastifyInstance } from "fastify";
import { requireSession } from "../auth/requireSession.js";
import type { SessionStore } from "../auth/sessionStore.js";
import { MemoryEraStore } from "./store.js";
import type { EraScopedView } from "./types.js";

export interface ErasRouteOptions {
  sessionStore: SessionStore;
  eraStore: MemoryEraStore;
  now: () => Date;
}

/** Registers `apps/api/eras`. Declaring an era requires a
 * session (it's the owner's own claim about their track); reading the
 * timeline needs no session — it's part of the public profile. */
export function registerErasRoutes(app: FastifyInstance, options: ErasRouteOptions): void {
  app.post<{ Params: { trackId: string }; Body: { label: string; description: string; lifetimeStartMs: number } }>("/tracks/:trackId/eras", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    const { label, description, lifetimeStartMs } = request.body;
    const era = {
      eraId: createHash("sha256").update(`${request.params.trackId}:${label}:${options.now().toISOString()}`).digest("hex").slice(0, 16),
      label,
      descriptionDigest: createHash("sha256").update(description).digest("hex"),
      startMs: options.now().getTime(),
      declaredBy: session.address,
    };
    const timeline = options.eraStore.addEra(request.params.trackId, lifetimeStartMs, era);
    return reply.code(201).send(timeline);
  });

  // The full lifetime, every declared era included — never partial.
  app.get<{ Params: { trackId: string } }>("/tracks/:trackId/eras", async (request, reply) => {
    const timeline = options.eraStore.getTimeline(request.params.trackId);
    if (!timeline) {
      return reply.code(404).send({ error: "not_found" });
    }
    return timeline;
  });

  // An era-scoped excerpt — always paired with the real lifetimeStartMs
  // and an explicit isExcerpt flag, per proofs.md: never label a
  // favorable cutout as the lifetime.
  app.get<{ Params: { trackId: string; eraId: string } }>("/tracks/:trackId/eras/:eraId", async (request, reply) => {
    const timeline = options.eraStore.getTimeline(request.params.trackId);
    const era = timeline?.eras.find((candidate) => candidate.eraId === request.params.eraId);
    if (!timeline || !era) {
      return reply.code(404).send({ error: "not_found" });
    }
    const view: EraScopedView = { era, isExcerpt: true, lifetimeStartMs: timeline.lifetimeStartMs };
    return view;
  });
}
