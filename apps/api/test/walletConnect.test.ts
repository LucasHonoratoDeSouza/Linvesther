import { chmodSync, existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { buildApp } from "../src/app.js";
import { MemoryNonceStore } from "../src/auth/nonceStore.js";
import { MemorySessionStore } from "../src/auth/sessionStore.js";

// The worker is replaced by a script that records what it is asked and answers
// like the real one, so the route's own checks are what is being tested.
function recordingWorker() {
  const dir = mkdtempSync(join(tmpdir(), "wallet-worker-"));
  const log = join(dir, "calls.log");
  const path = join(dir, "worker.sh");
  writeFileSync(path, `#!/bin/sh\ncat >> ${log}\necho >> ${log}\necho '{"ok":true,"connectionId":"c1","summary":{"tradesFetched":0,"flowsFetched":3,"catalogSymbolsFetched":0}}'\n`);
  chmodSync(path, 0o755);
  return { path, calls: () => (existsSync(log) ? readFileSync(log, "utf8").split("\n").filter(Boolean) : []) };
}

const ME = "0x1111111111111111111111111111111111111111" as const;
const DOMAIN = "linvesther.test";

async function setup(options: { verifyContract?: (input: { chainId: number; address: `0x${string}`; message: string; signature: `0x${string}` }) => Promise<boolean>; clock?: { t: number } } = {}) {
  const sessionStore = new MemorySessionStore();
  const session = await sessionStore.create(ME);
  const worker = recordingWorker();
  const clock = options.clock ?? { t: Date.now() };
  const nonces = new MemoryNonceStore(() => clock.t);
  const app = buildApp({
    domain: DOMAIN,
    sessionStore,
    binanceWorkerBinaryPath: worker.path,
    now: () => new Date(clock.t),
    walletProof: { domain: DOMAIN, uri: `https://${DOMAIN}`, nonces, verifyContract: options.verifyContract },
  });
  await app.ready();
  const cookies = { sid: session.id };
  const challenge = (accountId: string, address: string, chainId = 8453) =>
    app.inject({ method: "POST", url: `/accounts/${accountId}/wallet-challenge`, cookies, payload: { address, chainId } });
  const connect = (accountId: string, message: string, signature: string) =>
    app.inject({ method: "POST", url: `/accounts/${accountId}/wallet-connection`, cookies, payload: { message, signature, label: "Main" } });
  return { app, cookies, worker, challenge, connect, clock };
}

describe("connecting a wallet by proving ownership", () => {
  it("connects an address whose owner signs the challenge, and only then", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const { challenge, connect, worker } = await setup();
    const { message } = (await challenge(ME, owner.address)).json();
    expect(message).toContain("does not move funds");
    const signature = await owner.signMessage({ message });
    const response = await connect(ME, message, signature);
    expect(response.statusCode).toBe(201);
    const [call = ""] = worker.calls();
    expect(JSON.parse(call)).toMatchObject({ exchange: "wallet", accountId: ME, address: owner.address, label: "Main" });
  });

  it("never sends the address back in any response", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const { challenge, connect } = await setup();
    const issued = await challenge(ME, owner.address);
    const { message } = issued.json();
    const done = await connect(ME, message, await owner.signMessage({ message }));
    for (const body of [done.body, done.headers["set-cookie"]?.toString() ?? ""]) {
      expect(body.toLowerCase()).not.toContain(owner.address.toLowerCase().slice(2));
    }
  });

  it("refuses a signature by someone who does not hold the address", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const thief = privateKeyToAccount(generatePrivateKey());
    const { challenge, connect, worker } = await setup();
    const { message } = (await challenge(ME, owner.address)).json();
    const response = await connect(ME, message, await thief.signMessage({ message }));
    expect(response.statusCode).toBe(422);
    expect(response.json().reason).toBe("unsupported_wallet");
    expect(worker.calls()).toEqual([]);
  });

  it("accepts a smart-contract wallet only when the network confirms it", async () => {
    const holder = "0x3333333333333333333333333333333333333333" as const;
    const seen: number[] = [];
    const { challenge, connect, worker } = await setup({ verifyContract: async ({ chainId }) => (seen.push(chainId), true) });
    const { message } = (await challenge(ME, holder, 10)).json();
    expect((await connect(ME, message, "0x" + "ab".repeat(65))).statusCode).toBe(201);
    expect(seen).toEqual([10]);
    expect(worker.calls()).toHaveLength(1);
    const refused = await setup({ verifyContract: async () => false });
    const again = (await refused.challenge(ME, holder)).json().message;
    const response = await refused.connect(ME, again, "0x" + "ab".repeat(65));
    expect(response.json().reason).toBe("bad_signature");
  });

  it("uses a challenge once", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const { challenge, connect, worker } = await setup();
    const { message } = (await challenge(ME, owner.address)).json();
    const signature = await owner.signMessage({ message });
    expect((await connect(ME, message, signature)).statusCode).toBe(201);
    const replay = await connect(ME, message, signature);
    expect(replay.statusCode).toBe(422);
    expect(replay.json().reason).toBe("replayed");
    expect(worker.calls()).toHaveLength(1);
  });

  it("refuses a challenge that has expired", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const { challenge, connect, clock } = await setup();
    const { message } = (await challenge(ME, owner.address)).json();
    const signature = await owner.signMessage({ message });
    clock.t += 6 * 60_000;
    expect((await connect(ME, message, signature)).json().reason).toBe("expired");
  });

  it("refuses a challenge made for another account of the same person", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const { challenge, connect } = await setup();
    const { message } = (await challenge(`${ME}_one`, owner.address)).json();
    const response = await connect(`${ME}_two`, message, await owner.signMessage({ message }));
    expect(response.json().reason).toBe("wrong_account");
  });

  it("refuses a message made for another host, or altered after it was issued", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const { challenge, connect } = await setup();
    const { message } = (await challenge(ME, owner.address)).json();
    const foreign = message.replace(DOMAIN, "evil.example");
    expect((await connect(ME, foreign, await owner.signMessage({ message: foreign }))).json().reason).toBe("wrong_domain");
    const altered = message.replace("Link this wallet", "Approve this spender");
    expect((await connect(ME, altered, await owner.signMessage({ message: altered }))).json().reason).toBe("malformed");
  });

  it("needs a session that owns the account", async () => {
    const owner = privateKeyToAccount(generatePrivateKey());
    const { app, cookies } = await setup();
    const anonymous = await app.inject({ method: "POST", url: `/accounts/${ME}/wallet-challenge`, payload: { address: owner.address, chainId: 1 } });
    expect(anonymous.statusCode).toBe(401);
    const someoneElse = await app.inject({ method: "POST", url: `/accounts/0x2222222222222222222222222222222222222222/wallet-challenge`, cookies, payload: { address: owner.address, chainId: 1 } });
    expect(someoneElse.statusCode).toBe(403);
  });

  it("rejects malformed input", async () => {
    const { challenge, connect } = await setup();
    expect((await challenge(ME, "not-an-address")).statusCode).toBe(400);
    expect((await connect(ME, "garbage", "0x00")).statusCode).toBe(422);
  });

  it("is off when no proof is configured", async () => {
    const sessionStore = new MemorySessionStore();
    const session = await sessionStore.create(ME);
    const app = buildApp({ domain: DOMAIN, sessionStore });
    await app.ready();
    const response = await app.inject({ method: "POST", url: `/accounts/${ME}/wallet-challenge`, cookies: { sid: session.id }, payload: { address: ME, chainId: 1 } });
    expect(response.statusCode).toBe(503);
  });
});
