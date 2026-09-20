import { useState } from "react";
import { BrokerLogo } from "./BrokerLogo";
import type { BrokerDefinition } from "./brokers";
import styles from "./portfolio.module.css";

/** The one form for connecting an account at any broker — the first one,
 * an additional one, or new credentials for an existing one. Its fields
 * come from the broker's definition. Credentials are only ever held in
 * this component's state until submitted, then cleared. */
export function ConnectForm({
  broker,
  askForName,
  submitLabel,
  busy,
  onSubmit,
  onCancel,
  framed = true,
}: {
  broker: BrokerDefinition;
  askForName: boolean;
  submitLabel: string;
  busy: boolean;
  onSubmit: (values: { label: string; credentials: Record<string, string> }) => Promise<boolean>;
  onCancel?: () => void;
  /** Wrapped in its own card (inline on the page) or bare (inside the popup). */
  framed?: boolean;
}) {
  const [label, setLabel] = useState("");
  const [credentials, setCredentials] = useState<Record<string, string>>({});
  const complete = broker.fields.every((field) => (credentials[field.key] ?? "").length > 0);

  const body = (
    <>
      <ul className={styles.steps}>
        {broker.instructions.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      {askForName && (
        <div className={styles.field}>
          <label className={styles.fieldLabel} htmlFor={`${broker.id}-account-name`}>
            Name this account (optional)
          </label>
          <input
            id={`${broker.id}-account-name`}
            data-testid={`${broker.id}-account-name-input`}
            type="text"
            placeholder="e.g. Long-term, Swing trading"
            maxLength={40}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </div>
      )}
      {broker.fields.map((field) => (
        <div className={styles.field} key={field.key}>
          <label className={styles.fieldLabel} htmlFor={`${broker.id}-${field.key}`}>
            {field.label}
          </label>
          {field.multiline ? (
            <textarea
              id={`${broker.id}-${field.key}`}
              data-testid={`${broker.id}-${field.key}-input`}
              autoComplete="off"
              spellCheck={false}
              rows={5}
              style={{ fontFamily: "monospace", fontSize: 12, resize: "vertical", ...(field.secret ? ({ WebkitTextSecurity: "disc" } as object) : {}) }}
              value={credentials[field.key] ?? ""}
              onChange={(e) => setCredentials((current) => ({ ...current, [field.key]: e.target.value }))}
            />
          ) : (
            <input
              id={`${broker.id}-${field.key}`}
              data-testid={`${broker.id}-${field.key}-input`}
              type={field.secret ? "password" : "text"}
              autoComplete="off"
              value={credentials[field.key] ?? ""}
              onChange={(e) => setCredentials((current) => ({ ...current, [field.key]: e.target.value }))}
            />
          )}
        </div>
      ))}
      <div style={{ display: "flex", gap: 8 }}>
        <button
          className={styles.btnPrimary}
          data-testid={`connect-${broker.id}-button`}
          disabled={busy || !complete}
          onClick={async () => {
            if (await onSubmit({ label, credentials })) {
              setLabel("");
              setCredentials({});
            }
          }}
        >
          {busy ? "Connecting…" : submitLabel}
        </button>
        {onCancel && (
          <button className={styles.btnSecondary} type="button" onClick={onCancel}>
            Cancel
          </button>
        )}
      </div>
    </>
  );

  if (!framed) return body;
  return (
    <div className={styles.card}>
      <div className={styles.cardTitleRow}>
        <div className={styles.cardTitle} style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <BrokerLogo brokerId={broker.id} size={28} /> {broker.name}
        </div>
        <span className={styles.badge}>Read-only</span>
      </div>
      {body}
    </div>
  );
}
