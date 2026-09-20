import type { FastifyRequest } from "fastify";
import type { Session, SessionStore } from "./sessionStore.js";

export async function requireSession(request: FastifyRequest, sessionStore: SessionStore): Promise<Session | null> {
  const sid = request.cookies.sid;
  if (!sid) return null;
  return (await sessionStore.get(sid)) ?? null;
}
