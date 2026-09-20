import type { BundleSourceData } from "./types.js";

export class MemoryExportSourceStore {
  private readonly records = new Map<string, BundleSourceData>();

  save(trackId: string, source: BundleSourceData): void {
    this.records.set(trackId, source);
  }

  get(trackId: string): BundleSourceData | undefined {
    return this.records.get(trackId);
  }
}
