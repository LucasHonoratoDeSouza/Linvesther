import type { FastifyError, FastifyInstance } from "fastify";

/** Client mistakes (bad JSON, a body that is too large) keep their status and
 * a short code. Anything else is a server fault: it is logged in full and the
 * client is told nothing about it, so an error message can never leak
 * internals or what was in the request. */
export function registerErrorHandler(app: FastifyInstance): void {
  app.setErrorHandler((error: FastifyError, request, reply) => {
    const status = typeof error.statusCode === "number" ? error.statusCode : 500;
    if (status >= 400 && status < 500) {
      return reply.code(status).send({ error: error.code ? String(error.code).toLowerCase() : "bad_request" });
    }
    request.log.error(error);
    console.error(error);
    return reply.code(500).send({ error: "internal_error" });
  });
}
