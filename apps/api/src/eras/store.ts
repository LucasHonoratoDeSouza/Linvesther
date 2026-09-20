import type { EraTimeline, StrategyEra } from "./types.js";

export class MemoryEraStore {
  private readonly timelines = new Map<string, EraTimeline>();

  /** Creates the timeline for a track the first time it's touched;
   * `lifetimeStartMs` is fixed then and never changed by any later
   * call. */
  private timelineFor(trackId: string, lifetimeStartMs: number): EraTimeline {
    const existing = this.timelines.get(trackId);
    if (existing) return existing;
    const created: EraTimeline = { lifetimeStartMs, eras: [] };
    this.timelines.set(trackId, created);
    return created;
  }

  /** Adds `era` to `trackId`'s timeline. This only ever appends —
   * there is no operation in this store that removes an era or moves
   * `lifetimeStartMs`, so a new era can never substitute for the
   * track's full history. */
  addEra(trackId: string, lifetimeStartMs: number, era: StrategyEra): EraTimeline {
    const timeline = this.timelineFor(trackId, lifetimeStartMs);
    const updated: EraTimeline = { lifetimeStartMs: timeline.lifetimeStartMs, eras: [...timeline.eras, era].sort((a, b) => a.startMs - b.startMs) };
    this.timelines.set(trackId, updated);
    return updated;
  }

  getTimeline(trackId: string): EraTimeline | undefined {
    return this.timelines.get(trackId);
  }
}
