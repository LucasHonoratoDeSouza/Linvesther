// Session and limits module, per the protocol specification's
// the protocol (SIWE domain/nonce/expiration/signer, replay must fail) and
// the protocol (payload/frequency limits rejected before expensive work).
// "Done when": SIWE replay/wrong domain, CSRF and cross-account access
// all fail; payload/quota are blocked before the expensive job runs.

import { randomUUID } from "node:crypto";
import fastifyCookie from "@fastify/cookie";
import fastifyCors from "@fastify/cors";
import type {
  AuthenticationResponseJSON,
  RegistrationResponseJSON,
} from "@simplewebauthn/server";
import Fastify, { type FastifyInstance } from "fastify";
import { concat, getAddress, keccak256, slice } from "viem";
import { RateLimiter, type RateLimit } from "./auth/rateLimiter.js";
import { registerErrorHandler } from "./security/errors.js";
import { registerResponseHeaders } from "./security/responseHeaders.js";
import { registerTrafficLimits } from "./security/trafficLimits.js";
import { requireSession } from "./auth/requireSession.js";
import { MemorySessionStore, SESSION_TTL_MS, type SessionStore } from "./auth/sessionStore.js";
import { credentialStore } from "./auth/identitySignature.js";
import {
  beginChallenge,
  verifySignature as verifyVaultSignature,
} from "./auth/vault.js";
import {
  beginAuthentication,
  beginRegistration,
  finishAuthentication,
  finishRegistration,
} from "./auth/webauthn.js";
import { MemoryDisclosureStore, type DisclosureStore } from "./claims/store.js";
import {
  registerClaimsRoutes,
  type ClaimsRouteOptions,
} from "./claims/routes.js";
import { registerLifecycleRoutes } from "./lifecycle/routes.js";
import type { ChainLifecycleDeps } from "./lifecycle/chainLifecycle.js";
import { registerPublicRoutes } from "./public/routes.js";
import { MemoryPublicTrackStore } from "./public/store.js";
import { registerExportsRoutes } from "./exports/routes.js";
import { MemoryExportSourceStore } from "./exports/store.js";
import { registerErasRoutes } from "./eras/routes.js";
import { MemoryEraStore } from "./eras/store.js";
import { registerMultiAccountRoutes } from "./multi-account/routes.js";
import { MemoryMultiAccountStore } from "./multi-account/store.js";
import { PublicProfileService } from "./profile/publicProfile.js";
import { registerProfileRoutes } from "./profile/routes.js";
import { MemorySettingsStore, type SettingsStore } from "./profile/store.js";
import { registerBinanceConnectRoutes } from "./binance-connect/routes.js";

export interface AppOptions {
  domain: string;
  /** Origins allowed to call this API with credentials (cookies) — the
   * web app's own origin in dev/e2e, never a wildcard, since the API
   * relies on cookie-based sessions. */
  corsOrigins?: string[];
  sessionStore?: SessionStore;
  /** Caps how often one session can start a real proof (CPU/RAM-heavy). */
  proofRateLimiter?: RateLimit;
  /** Where a claim is checked against the owner's current figures. Defaults to the combined record read from the connected accounts. */
  currentClaimMetrics?: ClaimsRouteOptions["currentMetrics"];
  /** Marks the session cookie `Secure` — set whenever the API is served over HTTPS. */
  secureCookies?: boolean;
  bodyLimitBytes?: number;
  /** How many reverse proxies sit in front of the API (0 when none). The client
   * address used for rate limits is taken from the nearest that many hops. */
  trustProxyHops?: number;
  /** Gas-paying actions the operator will fund per hour across all clients. */
  relayBudgetPerHour?: number;
  /** Where traffic-limit counts live: in memory unless a shared store is given. */
  limiterFactory?: (name: string, max: number, windowMs: number) => RateLimit;
  /** Scales every traffic limit. For a test environment only; leave unset in production. */
  limitMultiplier?: number;
  /** accountId -> the only address allowed to read it. */
  accountOwners?: Map<string, `0x${string}`>;
  /** Real on-chain identity lifecycle deps (Anvil/testnet + Postgres +
   * a relayer) — omitted, `/identities*` routes are not registered at
   * all, so tests unrelated to identity don't need to stand up a chain
   * and a database just to call `buildApp()`. */
  chainLifecycle?: ChainLifecycleDeps;
  disclosureStore?: DisclosureStore;
  publicTrackStore?: MemoryPublicTrackStore;
  exportSourceStore?: MemoryExportSourceStore;
  /** trackId -> the only address allowed to request its private export. */
  exportTrackOwners?: Map<string, `0x${string}`>;
  eraStore?: MemoryEraStore;
  multiAccountStore?: MemoryMultiAccountStore;
  /** trackId -> the only address allowed to manage its membership. */
  multiAccountTrackOwners?: Map<string, `0x${string}`>;
  /** Path to the compiled `binance-worker` binary (see
   * `services/collector/binance/worker`). Defaults to the debug build
   * path relative to the repo root, which is where this process is
   * normally started from. */
  binanceWorkerBinaryPath?: string;
  /** Each account's choices about its public profile (in memory unless given a durable store). */
  profileSettingsStore?: SettingsStore;
  now?: () => Date;
}

/** Design decision: a provisional owner address
 * for a P-256 credential, deterministic and recomputable from (qx, qy)
 * alone — no separate storage needed, and correctable later via
 * IdentityRegistry's owner rotation once a real smart-account
 * deployment exists. */
function deriveSubjectKey(qx: `0x${string}`, qy: `0x${string}`): `0x${string}` {
  return getAddress(slice(keccak256(concat([qx, qy])), 12));
}

/** Trusts only the nearest `hops` proxies, so a client cannot choose its own
 * address by sending a forged forwarding header. */
function trustHops(hops: number | undefined): false | ((address: string, hop: number) => boolean) {
  return hops && hops > 0 ? (_address, hop) => hop < hops : false;
}

export function buildApp(options: AppOptions): FastifyInstance {
  const sessionStore = options.sessionStore ?? new MemorySessionStore();
  const proofRateLimiter =
    options.proofRateLimiter ?? new RateLimiter(3, 10 * 60_000);
  const sessionCookie = {
    httpOnly: true,
    path: "/",
    sameSite: "lax" as const,
    secure: options.secureCookies ?? false,
    maxAge: SESSION_TTL_MS / 1000,
  };
  const accountOwners =
    options.accountOwners ?? new Map<string, `0x${string}`>();
  const disclosureStore =
    options.disclosureStore ?? new MemoryDisclosureStore();
  const publicTrackStore =
    options.publicTrackStore ?? new MemoryPublicTrackStore();
  const exportSourceStore =
    options.exportSourceStore ?? new MemoryExportSourceStore();
  const exportTrackOwners =
    options.exportTrackOwners ?? new Map<string, `0x${string}`>();
  const eraStore = options.eraStore ?? new MemoryEraStore();
  const multiAccountStore =
    options.multiAccountStore ?? new MemoryMultiAccountStore();
  const multiAccountTrackOwners =
    options.multiAccountTrackOwners ?? new Map<string, `0x${string}`>();
  const binanceWorkerBinaryPath =
    options.binanceWorkerBinaryPath ?? "services/target/debug/binance-worker";
  const now = options.now ?? (() => new Date());

  const app = Fastify({ bodyLimit: options.bodyLimitBytes ?? 1024 * 1024, trustProxy: trustHops(options.trustProxyHops) });
  // No logger is configured (tests would otherwise be noisy), so an
  // unhandled route error would otherwise vanish entirely — the client
  // only ever sees Fastify's generic "Internal Server Error" body, with
  // nothing server-side to diagnose it from.
  registerErrorHandler(app);
  registerResponseHeaders(app);
  registerTrafficLimits(app, { now, relayBudgetPerHour: options.relayBudgetPerHour, limitMultiplier: options.limitMultiplier });

  // A browser always names the page a state-changing request came from, so
  // a request from an origin the API does not serve is refused even though
  // it carries the person's cookie. Sign-in itself has no session yet.
  const allowedOrigins = new Set(options.corsOrigins ?? []);
  app.addHook("preHandler", async (request, reply) => {
    if (
      request.method === "GET" ||
      request.method === "HEAD" ||
      request.method === "OPTIONS"
    )
      return;
    const origin = request.headers.origin;
    if (origin && !allowedOrigins.has(origin)) {
      return reply.code(403).send({ error: "origin_not_allowed" });
    }
  });
  app.register(fastifyCookie);
  if (options.corsOrigins && options.corsOrigins.length > 0) {
    app.register(fastifyCors, {
      origin: options.corsOrigins,
      credentials: true,
      methods: ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"],
    });
  }
  if (options.chainLifecycle) {
    registerLifecycleRoutes(app, {
      sessionStore,
      chainLifecycle: options.chainLifecycle,
    });
  }
  registerPublicRoutes(app, { trackStore: publicTrackStore });
  registerExportsRoutes(app, {
    sessionStore,
    sourceStore: exportSourceStore,
    trackOwners: exportTrackOwners,
  });
  registerErasRoutes(app, { sessionStore, eraStore, now });
  registerMultiAccountRoutes(app, {
    sessionStore,
    multiAccountStore,
    trackOwners: multiAccountTrackOwners,
    now,
  });
  const profileSettings =
    options.profileSettingsStore ?? new MemorySettingsStore();
  const publicProfiles = new PublicProfileService({
    workerBinaryPath: binanceWorkerBinaryPath,
    settings: profileSettings,
    now,
  });
  registerBinanceConnectRoutes(app, {
    sessionStore,
    accountOwners,
    workerBinaryPath: binanceWorkerBinaryPath,
    proofRateLimiter,
    now,
    onAccountRemoved: (owner, accountId) =>
      publicProfiles.forgetAccount(owner, accountId),
  });
  registerProfileRoutes(app, {
    sessionStore,
    settings: profileSettings,
    chain: options.chainLifecycle,
    publicProfiles,
  });
  registerClaimsRoutes(app, {
    sessionStore,
    disclosureStore,
    now,
    collectorOrigin: () => publicProfiles.origin(),
    currentMetrics:
      options.currentClaimMetrics ??
      (async (owner) => {
        const track = await publicProfiles
          .combinedTrack(owner)
          .catch(() => null);
        if (!track) return null;
        const m = track.statement.metrics;
        return {
          metrics: {
            return: m.return === undefined ? undefined : Number(m.return),
            maxDrawdown:
              m.maxDrawdown === undefined ? undefined : Number(m.maxDrawdown),
            sharpe: m.sharpe === undefined ? undefined : Number(m.sharpe),
            winRate:
              m.winRate === undefined ? undefined : Number(m.winRate.value),
          },
          since: track.statement.since,
          computedAt: track.statement.computedAt,
        };
      }),
  });

  app.post<{ Body?: { userName?: string } }>(
    "/auth/webauthn/register-options",
    async (request) => {
      return beginRegistration(request.body?.userName ?? randomUUID());
    },
  );

  app.post<{ Body: { response: RegistrationResponseJSON } }>(
    "/auth/webauthn/register-verify",
    async (request, reply) => {
      const result = await finishRegistration(request.body.response);
      if (!result.verified || !result.qx || !result.qy) {
        return reply.code(401).send({ error: "signature_invalid" });
      }
      const subjectKey = deriveSubjectKey(result.qx, result.qy);
      await credentialStore.register(subjectKey, result.qx, result.qy, "webauthn");
      const session = await sessionStore.create(subjectKey);
      reply.setCookie("sid", session.id, sessionCookie);
      // The client only ever sees the WebAuthn credential's opaque id/rawId,
      // never (qx, qy) — it cannot re-derive subjectKey itself the way the
      // vault client can, so callers that need it (e.g. apps/web/app/portfolio,
      // which addresses Binance-connect routes by self-owned accountId) need
      // it returned here.
      return { csrfToken: session.csrfToken, address: subjectKey };
    },
  );

  app.post("/auth/webauthn/login-options", async () => {
    return beginAuthentication();
  });

  app.post<{ Body: { response: AuthenticationResponseJSON } }>(
    "/auth/webauthn/login-verify",
    async (request, reply) => {
      const result = await finishAuthentication(request.body.response);
      if (!result.verified || !result.qx || !result.qy) {
        return reply.code(401).send({ error: "signature_invalid" });
      }
      const subjectKey = deriveSubjectKey(result.qx, result.qy);
      const session = await sessionStore.create(subjectKey);
      reply.setCookie("sid", session.id, sessionCookie);
      return { csrfToken: session.csrfToken, address: subjectKey };
    },
  );

  // Lets a page check for an already-valid session (the "sid" cookie a
  // prior sign-in set) instead of unconditionally showing a sign-in
  // form — without this, navigating between pages that each manage
  // their own local "am I signed in" state re-prompts for credentials
  // even though the browser already holds a session that's still good.
  app.get("/auth/session", async (request, reply) => {
    const session = await requireSession(request, sessionStore);
    if (!session) {
      return reply.code(401).send({ error: "unauthenticated" });
    }
    return { csrfToken: session.csrfToken, address: session.address };
  });

  // Ends the current session — lets someone stuck signed in as the
  // wrong identity get back to the sign-in picker without waiting for
  // the server to restart.
  app.post("/auth/signout", async (request, reply) => {
    const sid = request.cookies?.sid;
    if (sid) await sessionStore.delete(sid);
    reply.clearCookie("sid", { path: "/" });
    return { ok: true };
  });

  app.post("/auth/vault/challenge", async () => ({
    challenge: await beginChallenge(),
  }));

  app.post<{
    Body: {
      qx: `0x${string}`;
      qy: `0x${string}`;
      challenge: string;
      signature: `0x${string}`;
    };
  }>("/auth/vault/verify", async (request, reply) => {
    const { qx, qy, challenge, signature } = request.body;
    const valid = await verifyVaultSignature(qx, qy, challenge, signature);
    if (!valid) {
      return reply.code(401).send({ error: "signature_invalid" });
    }
    const subjectKey = deriveSubjectKey(qx, qy);
    if (!(await credentialStore.get(subjectKey))) {
      await credentialStore.register(subjectKey, qx, qy, "vault");
    }
    const session = await sessionStore.create(subjectKey);
    reply.setCookie("sid", session.id, sessionCookie);
    return { csrfToken: session.csrfToken, address: subjectKey };
  });

  return app;
}
