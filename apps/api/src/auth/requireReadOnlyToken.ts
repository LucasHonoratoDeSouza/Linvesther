// Authenticates the read-only account API (`/mcp/*`). Parallel to
// `requireSession`, and deliberately disjoint from it:
//
// - this reads the `Authorization` header and never a cookie, so a
//   browser session cannot reach a `/mcp/*` route even when the browser
//   sends its cookie along;
// - `requireSession` reads the `sid` cookie and never a header, so a
//   read-only token cannot reach any route that changes state.
//
// Neither is a claim about the other's routes: the isolation is that
// each credential is only ever looked up by the code that owns it.

import type { FastifyRequest } from "fastify";
import {
  MAX_TOKEN_LENGTH,
  readOnlyTokenDigest,
  type ReadOnlyScope,
  type ReadOnlyTokenStore,
  type TokenRefusal,
} from "./readOnlyTokenStore.js";

/** Who is calling a read-only route. Carries no session, no CSRF token
 * and nothing that could authorize a write. */
export interface ReadOnlyPrincipal {
  tokenId: string;
  address: `0x${string}`;
  scope: ReadOnlyScope;
}

/** `Bearer <token>`, case-insensitively on the scheme (RFC 7235 says the
 * scheme is case-insensitive) and with the token bounded, so an
 * arbitrarily long header is refused before anything hashes it. */
function bearerToken(request: FastifyRequest): string | null {
  const header = request.headers.authorization;
  if (typeof header !== "string") return null;
  const space = header.indexOf(" ");
  if (space < 0) return null;
  if (header.slice(0, space).toLowerCase() !== "bearer") return null;
  const token = header.slice(space + 1).trim();
  return token.length > 0 && token.length <= MAX_TOKEN_LENGTH ? token : null;
}

/** The identity behind the request's bearer token, or why there is none.
 * `"unknown"` covers a missing, malformed and unrecognized token alike —
 * a caller learns nothing about which tokens exist. */
export async function requireReadOnlyToken(
  request: FastifyRequest,
  store: ReadOnlyTokenStore,
  now: Date,
): Promise<ReadOnlyPrincipal | TokenRefusal> {
  const presented = bearerToken(request);
  if (!presented) return "unknown";
  const result = await store.authenticate(presented, now);
  if (typeof result === "string") return result;
  return { tokenId: result.id, address: result.address, scope: result.scope };
}

/** Which client a read-only request counts against for traffic limits:
 * the token, not the address it belongs to and not the network address it
 * came from. One agent that runs hot cannot spend another's allowance,
 * and revoking a token drops its counter with it.
 *
 * The key is the token's hash, so no rate-limit table or log line ever
 * holds a usable credential. Requests with no token fall back to the
 * client address — they are refused anyway, and a limit per address is
 * what bounds guessing. */
export function readOnlyTrafficKey(request: FastifyRequest): string {
  const presented = bearerToken(request);
  if (!presented) return `address:${request.ip}`;
  return `token:${readOnlyTokenDigest(presented)}`;
}
