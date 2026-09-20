import Link from "next/link";

export const metadata = { title: "Overview" };

const tracks = [
  {
    href: "/docs/getting-started",
    title: "Get started",
    text: "Create an identity, connect an account and publish your first claim.",
  },
  {
    href: "/docs/concepts",
    title: "Understand it",
    text: "How performance is measured, what a claim is and what each guarantee means.",
  },
  {
    href: "/docs/api",
    title: "Build on it",
    text: "Read public profiles and claims over HTTP, and the contracts on Base.",
  },
];

export default function DocsOverview() {
  return (
    <>
      <span className="eyebrow">Documentation</span>
      <h1>Linvesther documentation</h1>
      <p className="document-lead">
        Linvesther turns the performance of your real accounts into public
        statements that anyone can check, without publishing a balance, a
        position or a trade.
      </p>

      <div className="docs-cards">
        {tracks.map((track) => (
          <Link key={track.href} href={track.href} className="docs-card">
            <h3>{track.title}</h3>
            <p>{track.text}</p>
          </Link>
        ))}
      </div>

      <section>
        <h2>The idea in four steps</h2>
        <ol>
          <li>
            <strong>Connect.</strong> You link your accounts with read-only
            credentials. Nothing can be traded or withdrawn.
          </li>
          <li>
            <strong>Measure.</strong> Linvesther computes percentages from your
            history: return, worst drop, risk-adjusted return and win rate.
            Every connected account is added together.
          </li>
          <li>
            <strong>Publish.</strong> Your public profile shows those
            percentages, or nothing at all if you turn on privacy mode. You can
            also share a single claim, such as “return at least 10%”.
          </li>
          <li>
            <strong>Verify.</strong> Ownership is recorded on a public
            blockchain, and results can be checked by anyone without access to
            your accounts.
          </li>
        </ol>
      </section>

      <section>
        <h2>What is public and what is not</h2>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Public</th>
              <th>Never published</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Percentages: return, max drawdown, Sharpe, win rate</td>
              <td>Balances and any absolute amount</td>
            </tr>
            <tr>
              <td>The date you connected and how long it has been tracked</td>
              <td>Positions, trades and order history</td>
            </tr>
            <tr>
              <td>Your public address and optional name and bio</td>
              <td>API keys, tokens and exchange identifiers</td>
            </tr>
            <tr>
              <td>Claims you choose to publish</td>
              <td>The exact figure behind a claim</td>
            </tr>
          </tbody>
        </table>
        <p>
          Read the <Link href="/whitepaper">whitepaper</Link> for the technology
          behind this, or continue to{" "}
          <Link href="/docs/getting-started">Getting started</Link>.
        </p>
      </section>
    </>
  );
}
