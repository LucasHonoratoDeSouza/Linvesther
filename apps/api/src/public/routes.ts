import type { FastifyInstance } from "fastify";
import { toPublicProjection } from "./projection.js";
import type { MemoryPublicTrackStore } from "./store.js";

export interface PublicRouteOptions {
  trackStore: MemoryPublicTrackStore;
}

/** Registers the public, unauthenticated projection endpoint. No
 * session is required — this is the public profile anyone can read —
 * and the response is always built through `toPublicProjection`, never
 * by forwarding the internal record. */
export function registerPublicRoutes(app: FastifyInstance, options: PublicRouteOptions): void {
  app.get<{ Params: { trackId: string } }>("/public/tracks/:trackId", async (request, reply) => {
    const record = options.trackStore.get(request.params.trackId);
    if (!record) {
      return reply.code(404).send({ error: "not_found" });
    }
    return toPublicProjection(record);
  });
}
