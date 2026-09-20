import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { buildApp } from "../src/app.js";
import { MemorySessionStore } from "../src/auth/sessionStore.js";
import type { BinancePerformance } from "../src/binance-connect/types.js";
import { combineReturns, maxDrawdown } from "../src/profile/combine.js";
import { buildStatement } from "../src/profile/statement.js";

const ME = "0x1111111111111111111111111111111111111111" as const;
const SOMEONE_ELSE = "0x2222222222222222222222222222222222222222" as const;
const CONNECTED_AT = Date.parse("2026-09-01T00:00:00Z");
const NOW = new Date("2026-09-11T00:00:00Z");
const COLLECTOR = "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134";

const performance: BinancePerformance = {
  dailyNav: [
    { dayStartMs: CONNECTED_AT, nav: "25000.5" },
    { dayStartMs: CONNECTED_AT + 86_400_000, nav: "27500.55" },
  ],
  dailyReturns: ["0.1", "0.1"],
  sharpe: null,
  sharpeUnavailable: "sample has 2 daily returns, fewer than the required minimum 30",
  sortino: null,
  sortinoUnavailable: "n/a",
  maxDrawdown: "0.05",
  cagr: null,
  cagrUnavailable: "n/a",
  winRate: { closedRoundTrips: 4, wins: 3, winRate: "0.75" },
  earliestReliableCheckpointMs: CONNECTED_AT,
  pnl: { positions: [], realized: "1", unrealized: "2", fees: "0" },
};

describe("buildStatement", () => {
  const build = (share: Parameters<typeof buildStatement>[0]["share"]) =>
    buildStatement({ address: ME, accountId: ME, trackName: "Main", share, performance, now: NOW });

  it("contains only the metrics that were chosen — nothing else about the account", () => {
    const { statement } = build(["return"]);
    expect(Object.keys(statement.metrics)).toEqual(["return"]);
    expect(statement.metrics.return).toBe("0.210000"); // 1.1 * 1.1 - 1, compounded
    const serialized = JSON.stringify(statement);
    for (const secret of ["25000", "27500", "nav", "pnl", "realized", "positions"]) {
      expect(serialized).not.toContain(secret);
    }
  });

  it("reports a chosen metric that can't be computed yet instead of guessing it", () => {
    const { statement, unavailable } = build(["sharpe", "maxDrawdown", "winRate"]);
    expect(unavailable).toEqual(["sharpe"]);
    expect(statement.metrics).toEqual({ maxDrawdown: "0.05", winRate: { value: "0.75", closedTrades: 4 } });
  });

  it("states when tracking started and for how long", () => {
    const { statement } = build(["return"]);
    expect(statement.since).toBe("2026-09-01T00:00:00.000Z");
    expect(statement.trackedDays).toBe(10);
  });
});

const series = {
  points: [
    { timeMs: CONNECTED_AT, nav: "25000.5", index: "1" },
    { timeMs: CONNECTED_AT + 3_600_000, nav: "26000.1", index: "1.02" },
  ],
  stepMs: 3_600_000,
  sinceMs: CONNECTED_AT,
};

/** A stand-in for the real worker: answers `list`, `performance` and
 * `series` the way the Rust binary does, and logs every call so the tests
 * can tell how often it was actually run. */
function fakeWorker(accountIds: string[]) {
  const dir = mkdtempSync(join(tmpdir(), "profile-worker-"));
  const path = join(dir, "worker.sh");
  const log = join(dir, "calls.log");
  const accounts = accountIds.map((accountId) => ({ accountId, label: null, connectedAtMs: CONNECTED_AT, status: "active" }));
  writeFileSync(
    path,
    `#!/bin/sh
sub="$1"
cat > /dev/null
echo "$sub" >> ${log}
case "$sub" in
  list) echo '${JSON.stringify({ ok: true, accounts })}' ;;
  performance) echo '${JSON.stringify({ ok: true, ...performance })}' ;;
  series) echo '${JSON.stringify({ ok: true, ...series })}' ;;
  collector-identity) echo '${JSON.stringify({ ok: true, fingerprintHex: COLLECTOR })}' ;;
esac
`,
  );
  chmodSync(path, 0o755);
  return { path, calls: (name: string) => readFileSync(log, "utf8").split("\n").filter((line) => line === name).length };
}

async function setup(accountIds: string[] = [ME], startAt = NOW) {
  let clock = startAt;
  const worker = fakeWorker(accountIds);
  const sessionStore = new MemorySessionStore();
  const app = buildApp({ domain: "localhost", sessionStore, binanceWorkerBinaryPath: worker.path, now: () => clock });
  await app.ready();
  return {
    app,
    worker,
    advance: (ms: number) => (clock = new Date(clock.getTime() + ms)),
    cookies: async (address: `0x${string}`) => ({ sid: (await sessionStore.create(address)).id }),
  };
}

const visit = (app: Awaited<ReturnType<typeof setup>>["app"], address: string) => app.inject({ method: "GET", url: `/public/profiles/${address}` });

describe("a public profile exists for every connected account, by default", () => {
  it("shows percentages at the owner's address with no setup at all — and never an absolute amount", async () => {
    const { app } = await setup();
    const response = await visit(app, ME.toLowerCase());
    expect(response.statusCode).toBe(200);
    const [track] = response.json().tracks;
    expect(track.statement.trackName).toBe("Main account");
    expect(track.statement.metrics).toEqual({ return: "0.020000", maxDrawdown: "0.000000", winRate: { value: "0.750000", closedTrades: 4 } });
    expect(track.unavailable).toEqual(["sharpe"]);
    const everything = JSON.stringify(response.json());
    for (const secret of ["25000", "27500", '"nav"', "pnl", "realized", "positions", "balance"]) {
      expect(everything).not.toContain(secret);
    }
  });

  it("adds every account into one track record instead of listing them apart", async () => {
    const { app } = await setup([ME, `${ME}_swing`]);
    const { tracks } = (await visit(app, ME)).json();
    expect(tracks).toHaveLength(1);
    expect(tracks[0].statement.trackName).toBe("All accounts");
    expect(tracks[0].statement.metrics.winRate).toEqual({ value: "0.750000", closedTrades: 8 });
  });

  it("answers 404 for an address with nothing connected, and 400 for a non-address", async () => {
    const { app } = await setup([]);
    expect((await visit(app, SOMEONE_ELSE)).statusCode).toBe(404);
    expect((await visit(app, "not-an-address")).statusCode).toBe(400);
  });

  it("doesn't re-run the worker for every visitor: one computation is shared for a minute, then refreshed", async () => {
    const { app, worker, advance } = await setup();
    await Promise.all([visit(app, ME), visit(app, ME), visit(app, ME)]);
    await visit(app, ME);
    expect(worker.calls("performance")).toBe(1);
    advance(61_000);
    await visit(app, ME);
    expect(worker.calls("performance")).toBe(2);
  });
});

describe("full privacy mode", () => {
  const put = (app: Awaited<ReturnType<typeof setup>>["app"], sid: { sid: string } | undefined, payload: object) =>
    app.inject({ method: "PUT", url: "/profile/settings", cookies: sid, payload });

  it("shows only that the profile is private — no numbers, no curve — until switched off", async () => {
    const { app, cookies } = await setup([ME, `${ME}_swing`]);
    const sid = (await cookies(ME));
    expect((await put(app, sid, { privacyMode: true })).json()).toMatchObject({ address: ME, privacyMode: true });
    const response = await visit(app, ME);
    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual({ address: ME, privacyMode: true, profile: null, tracks: [] });
    expect((await app.inject({ method: "GET", url: `/public/profiles/${ME}/series?range=max` })).statusCode).toBe(404);

    await put(app, sid, { privacyMode: false });
    expect((await visit(app, ME)).json().tracks).toHaveLength(1);
  });

  it("has no way to pick which accounts or numbers to show", async () => {
    const { app, cookies } = await setup();
    const sid = (await cookies(ME));
    expect((await app.inject({ method: "PUT", url: `/profile/settings/${ME}`, cookies: sid, payload: { metrics: ["return"] } })).statusCode).toBe(404);
    const [track] = (await visit(app, ME)).json().tracks;
    expect(Object.keys(track.statement.metrics).sort()).toEqual(["maxDrawdown", "return", "winRate"]);
  });

  it("requires signing in, and a true/false value", async () => {
    const { app, cookies } = await setup();
    expect((await put(app, undefined, { privacyMode: true })).statusCode).toBe(401);
    expect((await put(app, (await cookies(ME)), { privacyMode: "yes" })).statusCode).toBe(400);
    expect((await app.inject({ method: "GET", url: "/profile/settings" })).statusCode).toBe(401);
    expect((await app.inject({ method: "GET", url: "/profile/settings", cookies: await cookies(ME) })).json()).toEqual({ address: ME, privacyMode: false });
  });
});

describe("the public return curve", () => {
  const curve = (app: Awaited<ReturnType<typeof setup>>["app"], address: string, range = "max") =>
    app.inject({ method: "GET", url: `/public/profiles/${address}/series?range=${range}` });

  it("gives returns as percentages only — the account's value never leaves", async () => {
    const { app } = await setup();
    const response = await curve(app, ME);
    expect(response.statusCode).toBe(200);
    expect(response.json().points).toEqual([
      { timeMs: CONNECTED_AT, returnFraction: "0.000000" },
      { timeMs: CONNECTED_AT + 3_600_000, returnFraction: "0.020000" },
    ]);
    expect(JSON.stringify(response.json())).not.toContain('"nav"');
    expect(JSON.stringify(response.json())).not.toContain("25000");
  });

  it("is missing in privacy mode, for an empty address, and for a bad range", async () => {
    const { app, cookies } = await setup([ME]);
    expect((await curve(app, ME)).statusCode).toBe(200);
    await app.inject({ method: "PUT", url: "/profile/settings", cookies: await cookies(ME), payload: { privacyMode: true } });
    expect((await curve(app, ME)).statusCode).toBe(404);
    expect((await curve(app, ME, "9y")).statusCode).toBe(400);
    const empty = await setup([]);
    expect((await curve(empty.app, SOMEONE_ELSE)).statusCode).toBe(404);
  });
});

describe("the optional on-chain name and bio", () => {
  it("is unavailable, not broken, when no chain is configured", async () => {
    const { app, cookies } = await setup();
    const sid = (await cookies(ME));
    expect((await app.inject({ method: "GET", url: "/profile/identity", cookies: sid })).json()).toEqual({ enabled: false, active: false, profile: null });
    expect((await app.inject({ method: "POST", url: "/profile/identity/challenge", cookies: sid, payload: { name: "x" } })).statusCode).toBe(404);
    expect((await app.inject({ method: "PUT", url: "/profile/identity", cookies: sid, payload: {} })).statusCode).toBe(404);
  });

  it("requires signing in", async () => {
    const { app } = await setup();
    expect((await app.inject({ method: "GET", url: "/profile/identity" })).statusCode).toBe(401);
    expect((await app.inject({ method: "POST", url: "/profile/identity/challenge", payload: {} })).statusCode).toBe(401);
    expect((await app.inject({ method: "PUT", url: "/profile/identity", payload: {} })).statusCode).toBe(401);
  });
});

describe("combining accounts", () => {
  const at = (timeMs: number, nav: number, index: number) => ({ timeMs, nav: String(nav), index: String(index) });

  it("weights each account by what it held, and only from when it was connected", () => {
    const big = { sinceMs: 0, points: [at(0, 900, 1), at(10, 990, 1.1), at(20, 990, 1.1)] };
    const small = { sinceMs: 10, points: [at(10, 100, 1), at(20, 50, 0.5)] };
    const curve = combineReturns([big, small]);
    expect(curve.map((p) => p.timeMs)).toEqual([0, 10, 20]);
    expect(curve[1]!.growth).toBeCloseTo(1.1); // only the big account existed
    // 990 at +0%, 100 at -50% → weighted -5.05%
    expect(curve[2]!.growth).toBeCloseTo(1.1 * (1 - 50 / 1090));
  });

  it("measures the worst fall from a peak", () => {
    expect(maxDrawdown([{ growth: 1 }, { growth: 1.2 }, { growth: 0.9 }, { growth: 1.1 }])).toBeCloseTo(0.25);
  });
});

describe("where the figures come from", () => {
  it("names the collector that signs them, on a profile and on its own", async () => {
    const { app } = await setup();
    expect((await visit(app, ME)).json().origin).toEqual({ mechanism: "A0", collector: COLLECTOR });
    const alone = await app.inject({ method: "GET", url: "/public/collector" });
    expect(alone.json()).toEqual({ mechanism: "A0", collector: COLLECTOR });
  });

  it("asks the worker once, not on every visit", async () => {
    const { app, worker } = await setup();
    await visit(app, ME);
    await visit(app, ME);
    await app.inject({ method: "GET", url: "/public/collector" });
    expect(worker.calls("collector-identity")).toBe(1);
  });

  it("says there is no collector when the instance has no signing key, rather than guessing", async () => {
    const app = buildApp({ domain: "localhost", binanceWorkerBinaryPath: "/nonexistent/worker", now: () => NOW });
    await app.ready();
    const response = await app.inject({ method: "GET", url: "/public/collector" });
    expect(response.json()).toEqual({ mechanism: "A0", collector: null });
  });
});
