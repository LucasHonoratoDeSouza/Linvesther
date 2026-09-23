import { SERIES_RANGES, type BinancePerformance, type BinanceSeries, type ConnectedAccount, type SeriesRange } from "../binance-connect/types.js";
import { invokeWorker } from "../binance-connect/worker.js";
import { atMostDaily, combineReturns, maxDrawdown, sharpe } from "./combine.js";
import { buildStatement } from "./statement.js";
import type { SettingsStore } from "./store.js";
import type { CollectorOrigin, ProfileStatement, ShareableMetric } from "./types.js";

/** Public reads must not turn every page view into a multi-second worker
 * run, so what a visitor sees is at most this old (and says when it was
 * computed). It only ever caches what visitors see — the owner's own
 * portfolio always reads live. */
const PUBLIC_TTL_MS = 60_000;

/** Public reads are keyed by whatever address a stranger asks about, so the
 * cache is bounded: without a cap, asking about many addresses would grow it
 * without limit. */
const MAX_CACHED_KEYS = 5_000;

class TtlCache<T> {
  private readonly entries = new Map<string, { value: T; expiresAt: number }>();
  private readonly inflight = new Map<string, Promise<T>>();

  constructor(private readonly now: () => Date) {}

  delete(key: string): void {
    this.entries.delete(key);
  }

  deleteWhere(matches: (key: string) => boolean): void {
    for (const key of this.entries.keys()) {
      if (matches(key)) this.entries.delete(key);
    }
  }

  private evict(): void {
    const now = this.now().getTime();
    for (const [key, entry] of this.entries) {
      if (entry.expiresAt <= now) this.entries.delete(key);
    }
    // Still over: drop the oldest entries (Map iterates in insertion order).
    for (const key of this.entries.keys()) {
      if (this.entries.size <= MAX_CACHED_KEYS) break;
      this.entries.delete(key);
    }
  }

  /** Returns a fresh-enough value, sharing one load between concurrent callers. */
  async get(key: string, load: () => Promise<T>): Promise<T> {
    const hit = this.entries.get(key);
    if (hit && hit.expiresAt > this.now().getTime()) return hit.value;
    const running = this.inflight.get(key);
    if (running) return running;
    const started = load()
      .then((value) => {
        this.entries.set(key, { value, expiresAt: this.now().getTime() + PUBLIC_TTL_MS });
        if (this.entries.size > MAX_CACHED_KEYS) this.evict();
        return value;
      })
      .finally(() => this.inflight.delete(key));
    this.inflight.set(key, started);
    return started;
  }
}

export const ALL_ACCOUNTS = "all";
const SHAREABLE: ShareableMetric[] = ["return", "maxDrawdown", "sharpe", "winRate"];

export const defaultTrackName = (account: Pick<ConnectedAccount, "label">, index: number) =>
  account.label?.trim() || (index === 0 ? "Main account" : `Account ${index + 1}`);

export interface PublicTrack {
  statement: ProfileStatement;
  /** Metrics still shown as on but not computable yet (not enough history). */
  unavailable: ShareableMetric[];
}

/** Public series: percentages only — the account's value never leaves. */
export interface PublicSeries {
  /** Return since the start of this range, as a decimal fraction. */
  points: { timeMs: number; returnFraction: string }[];
  stepMs: number;
  sinceMs: number;
}

export class PublicProfileService {
  private readonly accountsCache: TtlCache<ConnectedAccount[]>;
  private readonly performanceCache: TtlCache<BinancePerformance>;
  private readonly seriesCache: TtlCache<BinanceSeries>;

  constructor(
    private readonly options: { workerBinaryPath: string; settings: SettingsStore; now: () => Date },
  ) {
    this.accountsCache = new TtlCache(options.now);
    this.performanceCache = new TtlCache(options.now);
    this.seriesCache = new TtlCache(options.now);
  }

  /** Drops what is remembered about a removed account, so the next public read
   * no longer includes it. */
  forgetAccount(address: string, accountId: string): void {
    this.accountsCache.delete(address.toLowerCase());
    const id = accountId.toLowerCase();
    this.performanceCache.delete(id);
    this.seriesCache.deleteWhere((key) => key.startsWith(`${id}|`));
  }

  private origin$: Promise<CollectorOrigin> | null = null;

  /** Which collector this instance reads exchanges as. The key never
   * changes while the process runs, so a successful answer is kept; a
   * failed lookup is reported as "no collector" and retried next time. */
  origin(): Promise<CollectorOrigin> {
    if (!this.origin$) {
      const lookup = invokeWorker<{ fingerprintHex: string | null }>({ binaryPath: this.options.workerBinaryPath }, "collector-identity", "{}")
        .then((identity): CollectorOrigin => ({ mechanism: "A0", collector: identity.fingerprintHex }))
        .catch((): CollectorOrigin | null => null);
      this.origin$ = lookup.then((origin) => {
        if (!origin) this.origin$ = null;
        return origin ?? { mechanism: "A0", collector: null };
      });
    }
    return this.origin$;
  }

  private worker<T>(subcommand: "list" | "performance" | "series", payload: object): Promise<T> {
    return invokeWorker<T>({ binaryPath: this.options.workerBinaryPath }, subcommand, JSON.stringify(payload));
  }

  /** Every active account connected under `address`, oldest first. */
  async accounts(address: string, fresh = false): Promise<ConnectedAccount[]> {
    const load = async () => {
      const { accounts } = await this.worker<{ accounts: ConnectedAccount[] }>("list", { ownerAddress: address });
      return accounts.filter((a) => a.status === "active");
    };
    return fresh ? load() : this.accountsCache.get(address.toLowerCase(), load);
  }

  /** Whether the address is in full privacy mode — nothing is shown at all. */
  private privacyMode(address: string) {
    return this.options.settings.get(address).then((s) => s.privacyMode);
  }

  /** What the public sees of an account's curve: a wallet's is coarsened to one point a day. */
  private async accountSeries(account: Pick<ConnectedAccount, "accountId" | "broker">, range: SeriesRange) {
    const series = await this.seriesCache.get(`${account.accountId.toLowerCase()}|${range}`, () => this.worker<BinanceSeries>("series", { accountId: account.accountId, range }));
    return account.broker === "wallet" ? atMostDaily(series) : series;
  }

  /** The one track anyone can see at `address`: every public account added
   * together, with every metric. */
  async tracks(address: `0x${string}`): Promise<PublicTrack[]> {
    if (await this.privacyMode(address)) return [];
    const track = await this.combinedTrack(address);
    return track ? [track] : [];
  }

  /** The same combined track, regardless of privacy mode — for the owner's
   * own use (for example checking a claim before signing it), never for
   * a public read. */
  async combinedTrack(address: `0x${string}`): Promise<PublicTrack | null> {
    const accounts = await this.accounts(address);
    if (accounts.length === 0) return null;

    const performances = await Promise.all(
      accounts.map((account) =>
        this.performanceCache.get(account.accountId.toLowerCase(), () => this.worker<BinancePerformance>("performance", { accountId: account.accountId })),
      ),
    );
    const curve = combineReturns(await Promise.all(accounts.map((account) => this.accountSeries(account, "max"))));
    const drawdown = maxDrawdown(curve);
    const ratio = sharpe(curve);
    const wins = performances.reduce((sum, p) => sum + (p.winRate?.wins ?? 0), 0);
    const closed = performances.reduce((sum, p) => sum + (p.winRate?.closedRoundTrips ?? 0), 0);
    const since = Math.min(...performances.map((p) => p.earliestReliableCheckpointMs ?? this.options.now().getTime()));

    const combined = {
      dailyReturns: curve.slice(1).map((point, i) => String(point.growth / curve[i]!.growth - 1)),
      maxDrawdown: curve.length > 1 ? drawdown.toFixed(6) : null,
      sharpe: ratio === null ? null : ratio.toFixed(6),
      winRate: closed > 0 ? { closedRoundTrips: closed, wins, winRate: (wins / closed).toFixed(6) } : null,
      earliestReliableCheckpointMs: since,
    };
    const share = SHAREABLE;
    const { statement, unavailable } = buildStatement({
      address,
      accountId: ALL_ACCOUNTS,
      trackName: accounts.length === 1 ? defaultTrackName(accounts[0]!, 0) : "All accounts",
      share,
      performance: combined,
      now: this.options.now(),
    });
    return { statement, unavailable };
  }

  /** The combined return curve, or `null` when nothing is public at that
   * address (never confirming a private account exists). */
  async series(address: `0x${string}`, range: SeriesRange): Promise<PublicSeries | null> {
    if (!(SERIES_RANGES as readonly string[]).includes(range)) return null;
    if (await this.privacyMode(address)) return null;
    const accounts = await this.accounts(address);
    if (accounts.length === 0) return null;

    const all = await Promise.all(accounts.map((account) => this.accountSeries(account, range)));
    const curve = combineReturns(all);
    return {
      points: curve.map((p) => ({ timeMs: p.timeMs, returnFraction: (p.growth - 1).toFixed(6) })),
      stepMs: Math.min(...all.map((s) => s.stepMs)),
      sinceMs: Math.min(...all.map((s) => s.sinceMs)),
    };
  }
}
