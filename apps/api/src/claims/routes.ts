import type { FastifyInstance } from "fastify";
import { requireSession } from "../auth/requireSession.js";
import type { IdentityCredentialProof } from "../auth/identitySignature.js";
import type { SessionStore } from "../auth/sessionStore.js";
import { checkAudience, checkExpiry, verifyConsent } from "./disclosure.js";
import { checkClaims, type ClaimMetrics } from "./evaluate.js";
import type { DisclosureStore } from "./store.js";
import type { CollectorOrigin } from "../profile/types.js";
import type { ClaimSet } from "./types.js";

export interface ClaimsRouteOptions {
  sessionStore: SessionStore;
  disclosureStore: DisclosureStore;
  now: () => Date;
  /** The owner's current combined figures (or `null` with nothing connected).
   * When present, a claim is only accepted if it is true of them. */
  /** Which collector the figures behind a claim come from. */
  collectorOrigin?: () => Promise<CollectorOrigin>;
  currentMetrics?: (owner: `0x${string}`) => Promise<{ metrics: ClaimMetrics; since: string; computedAt: string } | null>;
}

/** Registers the claims routes: publishing a signed claim set, gated by
 * exact-content consent and a check that it is true of the owner's current
 * figures; reading one back, gated by expiration and audience. */
export function registerClaimsRoutes(app: FastifyInstance, options: ClaimsRouteOptions): void {
  const { sessionStore, disclosureStore, now } = options;

  app.post<{ Body: { claimSet: ClaimSet; credential: IdentityCredentialProof } }>("/claims/disclose", async (request, reply) => {
    const session = await requireSession(request, sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    const { claimSet, credential } = request.body;

    if (options.currentMetrics) {
      const current = await options.currentMetrics(session.address);
      if (!current) return reply.code(400).send({ error: "nothing_to_prove" });
      const check = checkClaims(claimSet.claims, current.metrics);
      if (!check.ok) return reply.code(400).send({ error: check.error });
    }

    const result = await verifyConsent(claimSet, credential, session.address, now());
    if (!result.ok) {
      return reply.code(400).send({ error: result.error });
    }
    await disclosureStore.save(result.digest, { claimSet, credential, owner: session.address, publishedAt: now().toISOString() });
    return reply.code(201).send({ digest: result.digest });
  });

  // What the owner can claim right now: their own combined figures.
  app.get("/claims/context", async (request, reply) => {
    const session = await requireSession(request, sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    const current = options.currentMetrics ? await options.currentMetrics(session.address) : null;
    return { address: session.address, current };
  });

  app.get("/claims/mine", async (request, reply) => {
    const session = await requireSession(request, sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    const claims = (await disclosureStore.listByOwner(session.address))
      .map(({ digest, record }) => ({ digest, claimSet: record.claimSet, publishedAt: record.publishedAt ?? null }));
    return { claims };
  });

  // Public read of a shared claim: what was claimed, by whom, until when.
  app.get<{ Params: { digest: string } }>("/public/claims/:digest", async (request, reply) => {
    const record = await disclosureStore.get(request.params.digest);
    if (!record || record.claimSet.audience !== "public") return reply.code(404).send({ error: "not_found" });
    if (checkExpiry(record.claimSet, now())) return reply.code(410).send({ error: "expired" });
    return {
      digest: request.params.digest,
      owner: record.owner,
      claimSet: record.claimSet,
      publishedAt: record.publishedAt ?? null,
      verification: "owner_signed_collector_attested",
      origin: options.collectorOrigin ? await options.collectorOrigin() : { mechanism: "A0", collector: null },
    };
  });

  app.get<{ Params: { digest: string }; Querystring: { audience?: string } }>("/claims/:digest", async (request, reply) => {
    const record = await disclosureStore.get(request.params.digest);
    if (!record) {
      return reply.code(404).send({ error: "not_found" });
    }

    const expiryError = checkExpiry(record.claimSet, now());
    if (expiryError) {
      return reply.code(410).send({ error: expiryError });
    }

    const audienceError = checkAudience(record.claimSet, request.query.audience ?? "");
    if (audienceError) {
      return reply.code(403).send({ error: audienceError });
    }

    return record.claimSet;
  });
}
