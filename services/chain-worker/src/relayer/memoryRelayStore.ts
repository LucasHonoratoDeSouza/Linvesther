import type { RelayRecord, RelayStore } from "./types.js";

export class MemoryRelayStore implements RelayStore {
  private readonly records = new Map<string, RelayRecord>();

  find(dedupKey: string): RelayRecord | undefined {
    return this.records.get(dedupKey);
  }

  save(record: RelayRecord): void {
    this.records.set(record.dedupKey, record);
  }
}
