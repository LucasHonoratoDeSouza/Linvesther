import type { InternalTrackRecord } from "./internal.js";

export class MemoryPublicTrackStore {
  private readonly records = new Map<string, InternalTrackRecord>();

  save(record: InternalTrackRecord): void {
    this.records.set(record.trackId, record);
  }

  get(trackId: string): InternalTrackRecord | undefined {
    return this.records.get(trackId);
  }
}
