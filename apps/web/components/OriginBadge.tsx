import type { CollectorOrigin } from "../lib/api";
import { assessOrigin } from "../lib/collectorTrust";

/** Says whose data a public number rests on, so a self-run instance can never
 * pass for a trusted one. */
export function OriginBadge({
  origin,
}: {
  origin: CollectorOrigin | undefined;
}) {
  const trust = assessOrigin(origin);
  const label =
    trust.kind === "trusted"
      ? `Attested by ${trust.name}`
      : trust.kind === "self-attested"
        ? "Self-attested"
        : "Source not identified";
  const explanation =
    trust.kind === "trusted"
      ? "Read from the exchange by a collector on the list of trusted collectors."
      : trust.kind === "self-attested"
        ? "Read from the exchange by a collector that is not on the list of trusted collectors. These figures are only as trustworthy as whoever runs it."
        : "This instance did not say which collector reads the exchange.";
  return (
    <a
      href="/docs/concepts#origin"
      title={explanation}
      data-testid="origin-badge"
      data-origin={trust.kind}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 8,
        padding: "5px 12px",
        border: "1px solid var(--border)",
        borderRadius: 999,
        fontSize: 12,
        color:
          trust.kind === "trusted" ? "var(--accent)" : "var(--text-secondary)",
      }}
    >
      <span
        aria-hidden="true"
        style={{
          width: 6,
          height: 6,
          borderRadius: "50%",
          background: "currentColor",
        }}
      />
      {label}
    </a>
  );
}
