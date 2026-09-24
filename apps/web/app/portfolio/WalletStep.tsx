import { useEffect, useState } from "react";
import { requestWalletChallenge } from "../../lib/api";
import { discoverWallets, selectWalletAccount, signWalletMessage, type DiscoveredWallet } from "../../lib/wallets";
import type { BrokerDefinition } from "./brokers";
import styles from "./portfolio.module.css";

/** Connecting a wallet: choose one of the wallets installed in the browser, then sign a
 * message. Nothing else is asked of the wallet, and the address is never shown back. */
export function WalletStep({
  broker,
  busy,
  accountIdFor,
  onSubmit,
}: {
  broker: BrokerDefinition;
  busy: boolean;
  accountIdFor: (label: string) => string;
  onSubmit: (values: { label: string; credentials: Record<string, string> }, accountId: string) => Promise<boolean>;
}) {
  const [wallets, setWallets] = useState<DiscoveredWallet[]>([]);
  const [label, setLabel] = useState("");
  const [working, setWorking] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => discoverWallets((wallet) => setWallets((current) => (current.some((w) => w.info.uuid === wallet.info.uuid) ? current : [...current, wallet]))), []);

  async function choose(wallet: DiscoveredWallet) {
    setProblem(null);
    setWorking(wallet.info.uuid);
    try {
      const accountId = accountIdFor(label);
      const { address, chainId } = await selectWalletAccount(wallet.provider);
      const challenge = await requestWalletChallenge(accountId, address, chainId);
      if (!challenge.ok) {
        setProblem(`Could not start the connection: ${challenge.error}`);
        return;
      }
      const signature = await signWalletMessage(wallet.provider, address, challenge.data.message);
      await onSubmit({ label, credentials: { message: challenge.data.message, signature } }, accountId);
    } catch (cause) {
      // Closing the wallet's prompt is a choice, not a failure worth alarming anyone.
      const declined = typeof cause === "object" && cause !== null && (cause as { code?: number }).code === 4001;
      setProblem(declined ? "You closed the wallet before signing. Nothing was connected." : cause instanceof Error ? cause.message : "The wallet did not answer.");
    } finally {
      setWorking(null);
    }
  }

  return (
    <>
      <ul className={styles.steps}>
        {broker.instructions.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      <div className={styles.field}>
        <label className={styles.fieldLabel} htmlFor="wallet-account-name">
          Name this account (optional)
        </label>
        <input id="wallet-account-name" data-testid="wallet-account-name-input" type="text" placeholder="e.g. Cold storage, DeFi" maxLength={40} value={label} onChange={(e) => setLabel(e.target.value)} />
      </div>
      {problem && (
        <p className={styles.note} role="alert" data-testid="wallet-problem">
          {problem}
        </p>
      )}
      {wallets.length === 0 ? (
        <p className={styles.note} data-testid="wallet-none">
          No wallet was found in this browser. Install a wallet extension (for example MetaMask, Rabby or Coinbase Wallet) and reload this page.
        </p>
      ) : (
        wallets.map((wallet) => (
          <button
            type="button"
            key={wallet.info.uuid}
            className={styles.brokerOption}
            data-testid={`wallet-option-${wallet.info.rdns}`}
            disabled={busy || working !== null}
            onClick={() => choose(wallet)}
          >
            {/* eslint-disable-next-line @next/next/no-img-element -- the wallet's own icon arrives as a data: URI */}
            <img src={wallet.info.icon} alt="" width={40} height={40} />
            <span>
              <span className={styles.assetName}>{wallet.info.name}</span>
              <span className={styles.assetQty} style={{ display: "block" }}>
                {working === wallet.info.uuid ? "Waiting for your wallet…" : "Connect and sign"}
              </span>
            </span>
          </button>
        ))
      )}
    </>
  );
}
