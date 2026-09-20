import type { FastifyInstance } from "fastify";

/** Headers every API answer carries. Answers are personal or change with
 * who asks, so nothing is stored by a cache or guessed at by a browser. */
export function registerResponseHeaders(app: FastifyInstance): void {
  app.addHook("onSend", async (_request, reply) => {
    reply.header("x-content-type-options", "nosniff");
    reply.header("referrer-policy", "no-referrer");
    reply.header("cross-origin-resource-policy", "same-site");
    if (!reply.hasHeader("cache-control")) reply.header("cache-control", "no-store");
  });
}
