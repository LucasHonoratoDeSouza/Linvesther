// Managing the read-only tokens an identity has handed out.
//
// These routes are the opposite of the ones they create credentials
// for: they change state, so they need the browser session (passkey or
// password vault) and refuse a read-only token — minting a credential
// with a credential would make revocation meaningless, since a token
// could always issue its replacement.

import type { FastifyInstance } from "fastify";
import { requireSession } from "../auth/requireSession.js";
import type { SessionStore } from "../auth/sessionStore.js";
import {
  MAX_ACTIVE_TOKENS,
  TooManyReadOnlyTokensError,
  type ReadOnlyTokenStore,
} from "../auth/readOnlyTokenStore.js";
import { boundedString } from "../security/input.js";

export interface ApiTokenRouteOptions {
  sessionStore: SessionStore;
  tokenStore: ReadOnlyTokenStore;
  now?: () => Date;
}

const MAX_LABEL = 40;

export function registerApiTokenRoutes(app: FastifyInstance, options: ApiTokenRouteOptions): void {
  const clock = options.now ?? (() => new Date());

  // Every token this identity holds, revoked ones included, so a person
  // can see that a token they revoked really is gone.
  app.get("/api-tokens", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    return { tokens: await options.tokenStore.list(session.address), maxActive: MAX_ACTIVE_TOKENS };
  });

  // The one moment the token itself exists outside the agent that will
  // hold it. It is not stored, so it cannot be shown again.
  app.post<{ Body: { label?: unknown } }>("/api-tokens", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    const label = boundedString((request.body as { label?: unknown } | undefined)?.label, MAX_LABEL);
    if (!label) return reply.code(400).send({ error: "label_required" });
    try {
      const { token, record } = await options.tokenStore.issue(session.address, label, { now: clock() });
      return reply.code(201).send({ token, record });
    } catch (error) {
      if (error instanceof TooManyReadOnlyTokensError) {
        return reply.code(409).send({ error: "too_many_tokens", detail: error.message });
      }
      throw error;
    }
  });

  // Revoking is immediate and final: the next request carrying that
  // token is refused, and the token is never reinstated.
  app.delete<{ Params: { id: string } }>("/api-tokens/:id", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    const id = boundedString(request.params.id, 64);
    // A token belonging to somebody else is answered exactly like one
    // that does not exist, so the reply says nothing about who holds what.
    if (!id || !(await options.tokenStore.revoke(session.address, id, clock()))) {
      return reply.code(404).send({ error: "not_found" });
    }
    return { revoked: true };
  });
}
