import type { FastifyInstance } from "fastify";
import { requireSession } from "../auth/requireSession.js";
import type { SessionStore } from "../auth/sessionStore.js";
import { SERIES_RANGES, type SeriesRange } from "../binance-connect/types.js";
import { beginSetProfile, findOwnedIdentity, readIdentityProfile, setIdentityProfile, UnsupportedConfirmationMethodError, type ChainLifecycleDeps } from "../lifecycle/chainLifecycle.js";
import { LifecycleError } from "../lifecycle/types.js";
import { WorkerInvocationError } from "../binance-connect/worker.js";
import type { PublicProfileService } from "./publicProfile.js";
import type { SettingsStore } from "./store.js";

export interface ProfileRouteOptions {
  sessionStore: SessionStore;
  settings: SettingsStore;
  publicProfiles: PublicProfileService;
  /** Present only when the on-chain identity flow is configured. */
  chain?: ChainLifecycleDeps;
}

type SetProfileWire = {
  domain: { name: string; version: string; chainId: number; verifyingContract: `0x${string}` };
  types: { SetProfile: { name: string; type: string }[] };
  primaryType: "SetProfile";
  message: { identityId: `0x${string}`; name: string; bio: string; nonce: string; deadline: string };
};

const statusFor = (code: LifecycleError["code"]) => (code === "not_found" ? 404 : code === "forbidden" ? 403 : 400);

const ADDRESS = /^0x[0-9a-fA-F]{40}$/;

/** Every connected account is public by default, at its owner's address,
 * added together, as percentages only. The owner's one choice is full
 * privacy mode. Nothing here ever returns an absolute amount. */
export function registerProfileRoutes(app: FastifyInstance, options: ProfileRouteOptions): void {
  app.get("/profile/settings", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    return { address: session.address, ...(await options.settings.get(session.address)) };
  });

  app.put<{ Body: { privacyMode?: unknown } }>("/profile/settings", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    if (typeof request.body?.privacyMode !== "boolean") return reply.code(400).send({ error: "privacyMode_must_be_true_or_false" });
    const next = { privacyMode: request.body.privacyMode };
    await options.settings.save(session.address, next);
    return { address: session.address, ...next };
  });

  // The optional public name and bio, written on-chain by the owner.
  app.get("/profile/identity", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    if (!options.chain?.profileRegistry) return { enabled: false, active: false, profile: null };
    const identity = await findOwnedIdentity(options.chain, session.address);
    return { enabled: true, active: identity !== null, profile: await readIdentityProfile(options.chain, session.address) };
  });

  app.post<{ Body: { name?: unknown; bio?: unknown } }>("/profile/identity/challenge", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    if (!options.chain?.profileRegistry) return reply.code(404).send({ error: "profiles_not_enabled" });
    const { name = "", bio = "" } = request.body ?? {};
    if (typeof name !== "string" || typeof bio !== "string") return reply.code(400).send({ error: "name_and_bio_must_be_text" });
    try {
      const challenge = await beginSetProfile(options.chain, session.address, name.trim(), bio.trim());
      return { ...challenge, message: { ...challenge.message, nonce: challenge.message.nonce.toString(), deadline: challenge.message.deadline.toString() } };
    } catch (error) {
      if (error instanceof LifecycleError) return reply.code(statusFor(error.code)).send({ error: error.code, message: error.message });
      throw error;
    }
  });

  app.put<{ Body: { challenge: SetProfileWire; signature: `0x${string}` } }>("/profile/identity", async (request, reply) => {
    const session = await requireSession(request, options.sessionStore);
    if (!session) return reply.code(401).send({ error: "unauthenticated" });
    if (!options.chain?.profileRegistry) return reply.code(404).send({ error: "profiles_not_enabled" });
    const { challenge, signature } = request.body;
    try {
      const profile = await setIdentityProfile(
        options.chain,
        session.address,
        { ...challenge, message: { ...challenge.message, nonce: BigInt(challenge.message.nonce), deadline: BigInt(challenge.message.deadline) } },
        signature,
      );
      return { profile };
    } catch (error) {
      if (error instanceof LifecycleError) return reply.code(statusFor(error.code)).send({ error: error.code, message: error.message });
      if (error instanceof UnsupportedConfirmationMethodError) return reply.code(400).send({ error: "unsupported_method", message: error.message });
      throw error;
    }
  });

  // Public: anyone with the address. In privacy mode it says so and shows nothing.
  app.get<{ Params: { address: string } }>("/public/profiles/:address", async (request, reply) => {
    if (!ADDRESS.test(request.params.address)) return reply.code(400).send({ error: "not_an_address" });
    try {
      const address = request.params.address as `0x${string}`;
      if ((await options.settings.get(address)).privacyMode) return { address, privacyMode: true, profile: null, tracks: [] };
      const tracks = await options.publicProfiles.tracks(address);
      const profile = options.chain ? await readIdentityProfile(options.chain, address).catch(() => null) : null;
      if (tracks.length === 0 && !profile) return reply.code(404).send({ error: "no_public_profile" });
      return { address, privacyMode: false, profile, tracks, origin: await options.publicProfiles.origin() };
    } catch (error) {
      if (error instanceof WorkerInvocationError) return reply.code(502).send({ error: "profile_unavailable" });
      throw error;
    }
  });

  // Which collector this instance's figures are signed by, so a reader can
  // check it against a list of collectors they trust.
  app.get("/public/collector", async () => options.publicProfiles.origin());

  app.get<{ Params: { address: string }; Querystring: { range?: string } }>(
    "/public/profiles/:address/series",
    async (request, reply) => {
      if (!ADDRESS.test(request.params.address)) return reply.code(400).send({ error: "not_an_address" });
      const range = request.query.range ?? "max";
      if (!(SERIES_RANGES as readonly string[]).includes(range)) return reply.code(400).send({ error: "unknown_range" });
      try {
        const series = await options.publicProfiles.series(request.params.address as `0x${string}`, range as SeriesRange);
        if (!series) return reply.code(404).send({ error: "no_public_series" });
        return series;
      } catch (error) {
        if (error instanceof WorkerInvocationError) return reply.code(502).send({ error: "series_unavailable" });
        throw error;
      }
    },
  );
}
