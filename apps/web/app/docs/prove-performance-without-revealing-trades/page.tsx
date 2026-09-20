import Link from "next/link";

export const metadata = {
  title: "Prove performance without revealing trades",
  description:
    "How to show return and maximum drawdown to anyone while keeping balances, positions and trades private.",
};

export default function Page() {
  return (
    <>
      <span className="eyebrow">Guide</span>
      <h1>Prove performance without revealing trades</h1>
      <p className="document-lead">
        You can show how an account performed without showing what it holds or trades.
      </p>

      <section>
        <h2>What you can show</h2>
        <p>
          Return, maximum drawdown, and other ratios such as the Sharpe ratio and win rate, as percentages over a period. Return and maximum drawdown for an exchange account can be backed by a zero-knowledge proof today; the other metrics are measured from the same data and published with their coverage, but are not yet proven in zero knowledge.
        </p>
      </section>

      <section>
        <h2>What stays private</h2>
        <p>
          Balances, positions, individual trades, deposits and withdrawals, and the exchange account identifier. Deposits and withdrawals are taken out of the return so that adding money does not look like performance, and only the resulting percentage is published.
        </p>
      </section>

      <section>
        <h2>How you share it</h2>
        <p>
          Connect an account with a read-only key. Keys with trading or withdrawal permission are rejected, so Linvesther cannot move or trade your money. Your public profile then shows the combined record as percentages. You can choose which claims to make, and you can turn on privacy mode to hide the whole record.
        </p>
      </section>

      <section>
        <h2>How a reader checks it</h2>
        <p>
          A reader can compare the numbers with their own reading of the public API, verify a proof with the command-line verifier, and check which collector signed the source data against a list they trust. Each result also says how complete its data is; an unavailable metric is not zero.
        </p>
      </section>

      <section>
        <h2>Keep reading</h2>
        <ul>
          <li>
            <Link href="/docs/getting-started">Getting started</Link>
          </li>
          <li>
            <Link href="/docs/claims">Claims</Link>
          </li>
          <li>
            <Link href="/docs/verify">Verify a proof</Link>
          </li>
        </ul>
      </section>
    </>
  );
}
