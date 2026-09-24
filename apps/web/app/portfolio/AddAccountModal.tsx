import { useEffect, useState } from "react";
import { BROKER_KINDS, BROKERS, type BrokerDefinition } from "./brokers";
import { BrokerLogo } from "./BrokerLogo";
import { ConnectForm } from "./ConnectForm";
import { WalletStep } from "./WalletStep";
import styles from "./portfolio.module.css";

/** "Add account": pick where the account is, then connect it. */
export function AddAccountModal({
  busy,
  onConnect,
  accountIdFor,
  onClose,
}: {
  busy: boolean;
  /** `accountId` is given when the flow had to choose it before connecting (a wallet signs for one account). */
  onConnect: (broker: BrokerDefinition, values: { label: string; credentials: Record<string, string> }, accountId?: string) => Promise<boolean>;
  accountIdFor: (broker: BrokerDefinition, label: string) => string;
  onClose: () => void;
}) {
  const [broker, setBroker] = useState<BrokerDefinition | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className={styles.overlay} onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className={styles.modal} role="dialog" aria-modal="true" aria-labelledby="add-account-title" data-testid="add-account-modal">
        <div className={styles.modalHeader}>
          <h2 id="add-account-title">{broker ? `Connect ${broker.name}` : "Add an account"}</h2>
          <button type="button" className={styles.modalClose} aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>

        {!broker && (
          <>
            <p className={styles.note} style={{ marginTop: 0 }}>
              Where is the account you want to add? Linvesther only ever reads it — it can’t trade or move money.
            </p>
            {BROKER_KINDS.map((group) => {
              const brokers = BROKERS.filter((b) => b.kind === group.kind);
              return (
                <div key={group.kind} className={styles.brokerGroup}>
                  <div className={styles.sectionLabel}>{group.title}</div>
                  {brokers.map((b) => (
                    <button type="button" key={b.id} className={styles.brokerOption} data-testid={`broker-option-${b.id}`} onClick={() => setBroker(b)}>
                      <BrokerLogo brokerId={b.id} size={40} />
                      <span>
                        <span className={styles.assetName}>{b.name}</span>
                        <span className={styles.assetQty} style={{ display: "block" }}>
                          {b.summary}
                        </span>
                      </span>
                    </button>
                  ))}
                  {brokers.length === 0 && <p className={styles.note}>{group.empty}</p>}
                </div>
              );
            })}
          </>
        )}

        {broker && (
          <>
            <button type="button" className={styles.linkButton} style={{ padding: 0, marginBottom: 14 }} onClick={() => setBroker(null)}>
              ← Choose another
            </button>
            {broker.method === "wallet" ? (
              <WalletStep broker={broker} busy={busy} accountIdFor={(label) => accountIdFor(broker, label)} onSubmit={(values, accountId) => onConnect(broker, values, accountId)} />
            ) : (
              <ConnectForm broker={broker} askForName framed={false} submitLabel="Add account" busy={busy} onSubmit={(values) => onConnect(broker, values)} />
            )}
          </>
        )}
      </div>
    </div>
  );
}
