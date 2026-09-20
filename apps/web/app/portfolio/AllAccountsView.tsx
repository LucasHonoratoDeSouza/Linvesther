import { useState } from "react";
import type { BinanceNav, BinancePerformance, ConnectedAccount } from "../../lib/api";
import { usePrivacy } from "./privacy";
import { BrokerLogo } from "./BrokerLogo";
import { brokerById } from "./brokers";
import { dateLabel, money, priceDigits, quantity, signedMoney } from "./format";
import styles from "./portfolio.module.css";

export const accountName = (account: ConnectedAccount, index: number) =>
  account.label?.trim() || (index === 0 ? "Main account" : `Account ${index + 1}`);

/** Every connected account added up: the total, the combined profit, what is
 * held across all of them, and a row per account to open. */
export function AllAccountsView({
  accounts,
  navs,
  perfs,
  onOpen,
  onRename,
}: {
  accounts: ConnectedAccount[];
  navs: Record<string, BinanceNav>;
  perfs: Record<string, BinancePerformance>;
  onOpen: (accountId: string) => void;
  onRename: (accountId: string, label: string) => Promise<boolean>;
}) {
  const { mask, tone } = usePrivacy();
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  async function commit(accountId: string, current: string) {
    const wanted = draft.trim();
    setEditing(null);
    if (wanted && wanted !== current) await onRename(accountId, wanted);
  }
  const perfList = accounts.map((a) => perfs[a.accountId]).filter((p): p is BinancePerformance => Boolean(p));
  const realized = perfList.reduce((sum, p) => sum + Number(p.pnl.realized), 0);
  const unrealized = perfList.reduce((sum, p) => sum + Number(p.pnl.unrealized), 0);
  const fees = perfList.reduce((sum, p) => sum + Number(p.pnl.fees), 0);
  const profit = realized + unrealized;

  const totalNav = accounts.reduce((sum, a) => sum + Number(navs[a.accountId]?.nav ?? 0), 0);
  const holdings = new Map<string, { quantity: number; value: number; price: string }>();
  for (const account of accounts) {
    for (const asset of navs[account.accountId]?.assets ?? []) {
      const held = holdings.get(asset.asset) ?? { quantity: 0, value: 0, price: asset.price };
      held.quantity += Number(asset.quantity);
      held.value += Number(asset.value);
      holdings.set(asset.asset, held);
    }
  }

  return (
    <div>
      {perfList.length > 0 && (
        <div className={styles.card} data-testid="pnl-card">
          <div className={styles.cardTitleRow}>
            <div className={styles.cardTitle}>Profit across all accounts</div>
          </div>
          <div className={`${styles.pnlTotal} ${tone(profit)}`} data-testid="pnl-total">
            {mask(signedMoney(profit))} <sup>USD</sup>
          </div>
          <div className={styles.pnlBreakdown}>
            <div>
              <span>Realized</span>
              <strong className={tone(realized)}>{mask(signedMoney(realized))}</strong>
              <small>from positions you closed</small>
            </div>
            <div>
              <span>Unrealized</span>
              <strong className={tone(unrealized)}>{mask(signedMoney(unrealized))}</strong>
              <small>on what you still hold</small>
            </div>
            <div>
              <span>Fees paid</span>
              <strong>{mask(money(String(fees)))}</strong>
              <small>not deducted above</small>
            </div>
          </div>
          <p className={styles.note} style={{ marginTop: 14, marginBottom: 0 }}>
            Each account is counted from the moment you connected it. Open one to see its own performance and risk.
          </p>
        </div>
      )}

      <div className={styles.card}>
        <div className={styles.cardTitleRow}>
          <div className={styles.cardTitle}>Your accounts</div>
        </div>
        {accounts.map((account, index) => {
          const nav = navs[account.accountId];
          const perf = perfs[account.accountId];
          const accountProfit = perf ? Number(perf.pnl.realized) + Number(perf.pnl.unrealized) : null;
          return (
            <div
              role="button"
              tabIndex={0}
              className={styles.accountRow}
              key={account.accountId}
              onClick={() => editing !== account.accountId && onOpen(account.accountId)}
              onKeyDown={(e) => e.key === "Enter" && editing === null && onOpen(account.accountId)}
              data-testid="account-row"
            >
              <BrokerLogo brokerId={account.broker} />
              <div className={styles.assetInfo}>
                {editing === account.accountId ? (
                  <input
                    autoFocus
                    className={styles.renameInput}
                    data-testid="account-name-edit"
                    maxLength={40}
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onClick={(e) => e.stopPropagation()}
                    onBlur={() => commit(account.accountId, accountName(account, index))}
                    onKeyDown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter") commit(account.accountId, accountName(account, index));
                      if (e.key === "Escape") setEditing(null);
                    }}
                  />
                ) : (
                  <div className={styles.assetName}>
                    {accountName(account, index)}
                    <button
                      type="button"
                      className={styles.pencil}
                      aria-label="Rename account"
                      data-testid="rename-account"
                      onClick={(e) => {
                        e.stopPropagation();
                        setDraft(accountName(account, index));
                        setEditing(account.accountId);
                      }}
                    >
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                        <path d="M12 20h9" />
                        <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
                      </svg>
                    </button>
                  </div>
                )}
                <div className={styles.assetQty}>{brokerById(account.broker)?.name ?? account.broker} · connected {dateLabel(account.connectedAtMs)}</div>
              </div>
              <div className={styles.assetRight}>
                <div className={styles.assetUsd}>{nav ? `${mask(money(nav.nav))} ${nav.currency}` : "…"}</div>
                <div className={`${styles.assetQty} ${tone(accountProfit)}`}>
                  {accountProfit === null ? "…" : `${mask(signedMoney(accountProfit))} since connected`}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {holdings.size > 0 && (
        <div className={styles.card}>
          <div className={styles.cardTitleRow}>
            <div className={styles.cardTitle}>What you hold, in total</div>
          </div>
          {[...holdings.entries()]
            .sort((a, b) => b[1].value - a[1].value)
            .map(([asset, held]) => {
              const share = totalNav > 0 ? held.value / totalNav : 0;
              return (
                <div className={styles.assetRow} key={asset}>
                  <div className={styles.assetInfo}>
                    <div className={styles.assetName}>{asset}</div>
                    <div className={styles.assetQty}>
                      {mask(`${quantity(String(held.quantity))} × ${money(held.price, priceDigits(held.price))}`)}
                    </div>
                    <div className={styles.shareBar} aria-hidden="true">
                      <span style={{ width: `${Math.max(share * 100, 1)}%` }} />
                    </div>
                  </div>
                  <div className={styles.assetRight}>
                    <div className={styles.assetUsd}>{mask(money(String(held.value)))} USD</div>
                    <div className={styles.assetQty}>{(share * 100).toFixed(1)}% of portfolio</div>
                  </div>
                </div>
              );
            })}
        </div>
      )}
    </div>
  );
}
