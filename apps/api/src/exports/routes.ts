import type { FastifyInstance } from "fastify";
import { requireSession } from "../auth/requireSession.js";
import type { SessionStore } from "../auth/sessionStore.js";
import { buildPublicBundle } from "./publicBundle.js";
import { encryptPrivateExport } from "./privateExport.js";
import type { MemoryExportSourceStore } from "./store.js";

export interface ExportsRouteOptions {
  sessionStore: SessionStore;
  sourceStore: MemoryExportSourceStore;
  /** trackId -> the only address allowed to request its private export. */
  trackOwners: Map<string, `0x${string}`>;
}

/** Registers `apps/api/exports`. The public bundle needs no
 * session — it's the portable artifact anyone can verify offline — but
 * the private export requires the track's owner, since it carries
 * witness/raw data. */
export function registerExportsRoutes(app: FastifyInstance, options: ExportsRouteOptions): void {
  app.get<{ Params: { trackId: string } }>("/exports/:trackId/public", async (request, reply) => {
    const source = options.sourceStore.get(request.params.trackId);
    if (!source) {
      return reply.code(404).send({ error: "not_found" });
    }
    return buildPublicBundle(source);
  });

  app.post<{ Params: { trackId: string }; Body: { passphrase: string } }>("/exports/:trackId/private", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    const owner = options.trackOwners.get(request.params.trackId);
    if (!owner || owner.toLowerCase() !== session.address.toLowerCase()) {
      return reply.code(403).send({ error: "cross_account_access_denied" });
    }
    const source = options.sourceStore.get(request.params.trackId);
    if (!source) {
      return reply.code(404).send({ error: "not_found" });
    }
    return encryptPrivateExport(
      { witness: source.witness, rawPrivateSeries: source.rawPrivateSeries, privateSalts: source.privateSalts },
      request.body.passphrase,
    );
  });
}
