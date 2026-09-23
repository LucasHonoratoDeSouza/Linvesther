import type { FastifyInstance } from "fastify";
import { RateLimiter, type RateLimit } from "../auth/rateLimiter.js";

/** One kind of traffic and how much of it one client may send. */
interface Policy {
  name: string;
  matches: (method: string, path: string) => boolean;
  max: number;
  windowMs: number;
}

const CONNECT = /^\/accounts\/[^/]+\/(binance|coinbase|kraken|ibkr|wallet)-connection$|^\/accounts\/[^/]+\/wallet-challenge$/;
// Routes that make the server broadcast a transaction and pay its gas.
const RELAY =
  /^\/(identities|tracks)(\/|$)|^\/accounts\/[^/]+\/(activate|remove)(-challenge)?$|^\/profile\/identity/;

/** The first matching policy applies. Limits are per client address and are
 * sized well above normal use: they stop a script, not a person. */
export const DEFAULT_POLICIES: Policy[] = [
  // Signing in costs nothing, so it is where anyone can make new sessions.
  {
    name: "sign-in",
    matches: (m, p) =>
      m === "POST" && p.startsWith("/auth/") && p !== "/auth/signout",
    max: 30,
    windowMs: 60_000,
  },
  // Each connection calls an exchange with a stranger's credentials.
  {
    name: "connect",
    matches: (m, p) => m === "POST" && CONNECT.test(p),
    max: 10,
    windowMs: 10 * 60_000,
  },
  // Each of these costs the operator real gas.
  {
    name: "relay",
    matches: (m, p) => m !== "GET" && RELAY.test(p),
    max: 30,
    windowMs: 60 * 60_000,
  },
  // Unauthenticated, and a cache miss runs the worker.
  {
    name: "public",
    matches: (_m, p) => p.startsWith("/public/"),
    max: 120,
    windowMs: 60_000,
  },
  { name: "other", matches: () => true, max: 600, windowMs: 60_000 },
];

/** How many gas-paying actions the operator will fund in an hour, across
 * every client together. Per-client limits stop one script; this stops many,
 * so the relayer's balance cannot be drained by anyone. */
export const DEFAULT_RELAY_BUDGET_PER_HOUR = 300;

/** Refuses a client that sends more than its share, before the route runs. */
export function registerTrafficLimits(
  app: FastifyInstance,
  options: {
    now: () => Date;
    policies?: Policy[];
    relayBudgetPerHour?: number;
    limitMultiplier?: number;
    /** Where the counts live. In memory by default; a shared store makes the
     * limits hold across instances and restarts. */
    limiterFactory?: (name: string, max: number, windowMs: number) => RateLimit;
  },
): void {
  // Scales every limit. Only for a test environment that signs in far faster
  // than a person would.
  const scale =
    options.limitMultiplier !== undefined && options.limitMultiplier > 0
      ? options.limitMultiplier
      : 1;
  const makeLimiter =
    options.limiterFactory ??
    ((_name: string, max: number, windowMs: number) =>
      new RateLimiter(max, windowMs));
  const policies = (options.policies ?? DEFAULT_POLICIES).map((policy) => ({
    policy,
    limiter: makeLimiter(policy.name, policy.max * scale, policy.windowMs),
  }));
  const relayBudget = makeLimiter(
    "relay-budget",
    (options.relayBudgetPerHour ?? DEFAULT_RELAY_BUDGET_PER_HOUR) * scale,
    60 * 60_000,
  );
  app.addHook("onRequest", async (request, reply) => {
    if (request.method === "OPTIONS") return;
    const path = request.url.split("?")[0] ?? request.url;
    const match = policies.find(({ policy }) =>
      policy.matches(request.method, path),
    );
    // Counted only for requests the per-client limit lets through.
    const spendsGas = request.method !== "GET" && RELAY.test(path);
    if (!match) return;
    if (
      !(await match.limiter.allow(
        `${match.policy.name}:${request.ip}`,
        options.now().getTime(),
      ))
    ) {
      return reply
        .code(429)
        .header("retry-after", String(Math.ceil(match.policy.windowMs / 1000)))
        .send({ error: "rate_limited" });
    }
    if (
      spendsGas &&
      !(await relayBudget.allow("all", options.now().getTime()))
    ) {
      return reply
        .code(503)
        .header("retry-after", "3600")
        .send({ error: "relay_budget_exhausted" });
    }
  });
}
