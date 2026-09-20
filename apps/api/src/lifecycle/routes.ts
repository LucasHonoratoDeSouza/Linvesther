import type { FastifyInstance, FastifyReply } from "fastify";
import type {
  Account,
  ActivateAccountChallenge,
  ConfirmOwnerRotationChallenge,
  CreateTrackChallenge,
  Identity,
  RegisterAccountChallenge,
  RemoveAccountChallenge,
} from "@linvestherzk/chain-worker";
import { requireSession } from "../auth/requireSession.js";
import type { SessionStore } from "../auth/sessionStore.js";
import {
  beginActivateAccount,
  beginConfirmIdentity,
  beginCreateTrack,
  beginRegisterAccount,
  beginRemoveAccount,
  activateAccount,
  confirmIdentity,
  createIdentity,
  createTrack,
  findMyIdentity,
  registerAccount,
  removeAccount,
  type ChainLifecycleDeps,
} from "./chainLifecycle.js";
import { LifecycleError } from "./types.js";

// JSON has no BigInt type — ownerEpoch/nonce/deadline cross the wire as
// decimal strings, converted back to bigint on the way in. The digest a
// client hashes (hashTypedData) needs the real bigint values, not the
// wire strings, so this boundary has to be exact in both directions.
function serializeIdentity(identity: Identity) {
  return { ...identity, ownerEpoch: identity.ownerEpoch.toString() };
}

type BigintChallenge = { message: { ownerEpoch: bigint; nonce: bigint; deadline: bigint } & Record<string, unknown> };
type Wire<T extends BigintChallenge> = Omit<T, "message"> & {
  message: Omit<T["message"], "ownerEpoch" | "nonce" | "deadline"> & { ownerEpoch: string; nonce: string; deadline: string };
};

function serializeChallenge<T extends BigintChallenge>(challenge: T): Wire<T> {
  return {
    ...challenge,
    message: {
      ...challenge.message,
      ownerEpoch: challenge.message.ownerEpoch.toString(),
      nonce: challenge.message.nonce.toString(),
      deadline: challenge.message.deadline.toString(),
    },
  } as Wire<T>;
}

function deserializeChallenge<T extends BigintChallenge>(wire: Wire<T>): T {
  return {
    ...wire,
    message: {
      ...wire.message,
      ownerEpoch: BigInt(wire.message.ownerEpoch),
      nonce: BigInt(wire.message.nonce),
      deadline: BigInt(wire.message.deadline),
    },
  } as T;
}

export interface LifecycleRouteOptions {
  sessionStore: SessionStore;
  chainLifecycle: ChainLifecycleDeps;
}

function statusFor(code: LifecycleError["code"]): number {
  switch (code) {
    case "not_found":
      return 404;
    case "forbidden":
      return 403;
    case "invalid_state":
      return 409;
  }
}

/** Registers the owner-facing lifecycle commands against the real
 * on-chain identity flow (see chainLifecycle.ts) — `POST /identities`
 * starts creation (relayer becomes temporary owner, proposes the
 * session's account as the real one); `POST
 * /identities/:identityId/confirm-challenge` returns the exact EIP-712
 * payload to sign; `POST /identities/:identityId/confirm` relays that
 * signature and finalizes ownership. */
export function registerLifecycleRoutes(app: FastifyInstance, options: LifecycleRouteOptions): void {
  const { sessionStore, chainLifecycle } = options;

  async function withSession(reply: FastifyReply, request: Parameters<typeof requireSession>[0]) {
    const session = await requireSession(request, sessionStore);
    if (!session) {
      reply.code(401).send({ error: "unauthenticated" });
      return null;
    }
    return session;
  }

  function handleLifecycleError(reply: FastifyReply, error: unknown) {
    if (error instanceof LifecycleError) {
      return reply.code(statusFor(error.code)).send({ error: error.code, message: error.message });
    }
    throw error;
  }

  app.post("/identities", async (request, reply) => {
    const session = await withSession(reply, request);
    if (!session) return;
    try {
      const identity = await createIdentity(chainLifecycle, session.address);
      return reply.code(202).send(serializeIdentity(identity));
    } catch (error) {
      return handleLifecycleError(reply, error);
    }
  });

  // Whatever identity the signed-in session already owns on-chain, in
  // any state — so a page can resume onboarding at the right step
  // (confirm, or just show the dashboard) instead of offering to
  // create a second identity for someone who already has one.
  app.get("/identities/mine", async (request, reply) => {
    const session = await withSession(reply, request);
    if (!session) return;
    const identity = await findMyIdentity(chainLifecycle, session.address);
    return { identity: identity ? serializeIdentity(identity) : null };
  });

  app.post<{ Params: { identityId: string } }>("/identities/:identityId/confirm-challenge", async (request, reply) => {
    const session = await withSession(reply, request);
    if (!session) return;
    try {
      const challenge = await beginConfirmIdentity(chainLifecycle, request.params.identityId as `0x${string}`, session.address);
      return reply.send(serializeChallenge(challenge));
    } catch (error) {
      return handleLifecycleError(reply, error);
    }
  });

  app.post<{ Params: { identityId: string }; Body: { challenge: Wire<ConfirmOwnerRotationChallenge>; signature: `0x${string}` } }>(
    "/identities/:identityId/confirm",
    async (request, reply) => {
      const session = await withSession(reply, request);
      if (!session) return;
      try {
        const identity = await confirmIdentity(chainLifecycle, session.address, deserializeChallenge(request.body.challenge), request.body.signature);
        return reply.code(202).send(serializeIdentity(identity));
      } catch (error) {
        return handleLifecycleError(reply, error);
      }
    },
  );

  app.get<{ Params: { identityId: string } }>("/identities/:identityId", async (request, reply) => {
    const identity = await chainLifecycle.projectionStore.getIdentity(request.params.identityId as `0x${string}`);
    if (!identity) {
      return reply.code(404).send({ error: "not_found" });
    }
    return serializeIdentity(identity);
  });

  app.post<{
    Params: { identityId: string };
    Body: { financialProfileId: `0x${string}`; denominationCommitment: `0x${string}` };
  }>("/identities/:identityId/tracks/challenge", async (request, reply) => {
    const session = await withSession(reply, request);
    if (!session) return;
    try {
      const challenge = await beginCreateTrack(
        chainLifecycle,
        request.params.identityId as `0x${string}`,
        session.address,
        request.body.financialProfileId,
        request.body.denominationCommitment,
      );
      return reply.send(serializeChallenge(challenge));
    } catch (error) {
      return handleLifecycleError(reply, error);
    }
  });

  app.post<{ Params: { identityId: string }; Body: { challenge: Wire<CreateTrackChallenge>; signature: `0x${string}` } }>(
    "/identities/:identityId/tracks",
    async (request, reply) => {
      const session = await withSession(reply, request);
      if (!session) return;
      try {
        const result = await createTrack(chainLifecycle, session.address, deserializeChallenge(request.body.challenge), request.body.signature);
        return reply.code(202).send(result);
      } catch (error) {
        return handleLifecycleError(reply, error);
      }
    },
  );

  app.post<{
    Params: { trackId: string };
    Body: { venueId: `0x${string}`; authenticatedIdCommitment: `0x${string}`; environment: number };
  }>("/tracks/:trackId/accounts/challenge", async (request, reply) => {
    const session = await withSession(reply, request);
    if (!session) return;
    try {
      const challenge = await beginRegisterAccount(
        chainLifecycle,
        request.params.trackId as `0x${string}`,
        session.address,
        request.body.venueId,
        request.body.authenticatedIdCommitment,
        request.body.environment,
      );
      return reply.send(serializeChallenge(challenge));
    } catch (error) {
      return handleLifecycleError(reply, error);
    }
  });

  app.post<{ Params: { trackId: string }; Body: { challenge: Wire<RegisterAccountChallenge>; signature: `0x${string}` } }>(
    "/tracks/:trackId/accounts",
    async (request, reply) => {
      const session = await withSession(reply, request);
      if (!session) return;
      try {
        const account = await registerAccount(chainLifecycle, session.address, deserializeChallenge(request.body.challenge), request.body.signature);
        return reply.code(202).send(account satisfies Account);
      } catch (error) {
        return handleLifecycleError(reply, error);
      }
    },
  );

  app.post<{ Params: { accountId: string }; Body: { reasonHash: `0x${string}` } }>(
    "/accounts/:accountId/activate-challenge",
    async (request, reply) => {
      const session = await withSession(reply, request);
      if (!session) return;
      try {
        const challenge = await beginActivateAccount(chainLifecycle, request.params.accountId as `0x${string}`, session.address, request.body.reasonHash);
        // eligibleFrom is a bigint too, on top of the ownerEpoch/nonce/
        // deadline every challenge already has serialized generically.
        const wire = serializeChallenge(challenge);
        return reply.send({ ...wire, message: { ...wire.message, eligibleFrom: challenge.message.eligibleFrom.toString() } });
      } catch (error) {
        return handleLifecycleError(reply, error);
      }
    },
  );

  app.post<{ Params: { accountId: string }; Body: { challenge: Wire<ActivateAccountChallenge>; signature: `0x${string}` } }>(
    "/accounts/:accountId/activate",
    async (request, reply) => {
      const session = await withSession(reply, request);
      if (!session) return;
      try {
        const challenge = deserializeChallenge(request.body.challenge);
        challenge.message.eligibleFrom = BigInt(request.body.challenge.message.eligibleFrom as unknown as string);
        const account = await activateAccount(chainLifecycle, session.address, challenge, request.body.signature);
        return reply.code(202).send(account satisfies Account);
      } catch (error) {
        return handleLifecycleError(reply, error);
      }
    },
  );

  app.post<{ Params: { accountId: string }; Body: { reasonHash: `0x${string}` } }>(
    "/accounts/:accountId/remove-challenge",
    async (request, reply) => {
      const session = await withSession(reply, request);
      if (!session) return;
      try {
        const challenge = await beginRemoveAccount(chainLifecycle, request.params.accountId as `0x${string}`, session.address, request.body.reasonHash);
        return reply.send(serializeChallenge(challenge));
      } catch (error) {
        return handleLifecycleError(reply, error);
      }
    },
  );

  app.post<{ Params: { accountId: string }; Body: { challenge: Wire<RemoveAccountChallenge>; signature: `0x${string}` } }>(
    "/accounts/:accountId/remove",
    async (request, reply) => {
      const session = await withSession(reply, request);
      if (!session) return;
      try {
        const account = await removeAccount(chainLifecycle, session.address, deserializeChallenge(request.body.challenge), request.body.signature);
        return reply.code(202).send(account satisfies Account);
      } catch (error) {
        return handleLifecycleError(reply, error);
      }
    },
  );
}
