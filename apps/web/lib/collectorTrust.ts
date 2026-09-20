import trustList from "../../../trust/collectors.json";
import type { CollectorOrigin } from "./api";

interface ListedCollector {
  name: string;
  fingerprint: string;
  validFromMs?: number;
  validUntilMs?: number;
  revoked?: boolean;
}

export type OriginTrust =
  | { kind: "trusted"; name: string }
  | { kind: "self-attested"; fingerprint: string }
  | { kind: "unknown" };

/** Whether the collector behind some figures is on the list of collectors
 * this site ships with (`trust/collectors.json`). A collector that is not
 * listed is "self-attested": the figures are exactly as trustworthy as whoever
 * holds its key. This is a convenience label, not the strong check: for that,
 * verify the proof yourself with your own list. */
export function assessOrigin(
  origin: CollectorOrigin | undefined,
  at: number = Date.now(),
  collectors: ListedCollector[] = trustList.collectors,
): OriginTrust {
  if (!origin?.collector) return { kind: "unknown" };
  const fingerprint = origin.collector.toLowerCase();
  const listed = collectors.find(
    (entry) => entry.fingerprint.toLowerCase() === fingerprint,
  );
  if (!listed || listed.revoked) return { kind: "self-attested", fingerprint };
  if (
    (listed.validFromMs !== undefined && at < listed.validFromMs) ||
    (listed.validUntilMs !== undefined && at > listed.validUntilMs)
  ) {
    return { kind: "self-attested", fingerprint };
  }
  return { kind: "trusted", name: listed.name };
}
