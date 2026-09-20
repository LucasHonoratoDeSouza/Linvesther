import Link from "next/link";

export const metadata = { title: "How it works" };

const metrics = [
  [
    "Return",
    "How much the record grew since you connected, ignoring deposits and withdrawals.",
    "None",
  ],
  ["Max drawdown", "The biggest fall from a peak, as a percentage.", "None"],
  [
    "Sharpe ratio",
    "Return relative to how much it swung day to day (risk-free rate taken as zero).",
    "30 days",
  ],
  [
    "Win rate",
    "The share of closed trades that ended in profit.",
    "A closed trade",
  ],
];

const dimensions = [
  [
    "Origin",
    "Who supplied the data. A collector reads your exchange with a read-only key and signs what it saw; proofs name that collector. That is not the exchange signing the data.",
  ],
  ["Coverage", "Whether the data covers the period without unexplained gaps."],
  [
    "Calculation",
    "Whether the result comes with proof that it follows from the data by the published method.",
  ],
  ["Registry", "Whether the result is recorded on the public blockchain."],
  [
    "Availability",
    "Whether the evidence needed to check it can still be reached.",
  ],
];

export default function Concepts() {
  return (
    <>
      <span className="eyebrow">Concepts</span>
      <h1>How it works</h1>
      <p className="document-lead">
        What gets measured, what gets published, and what each guarantee means.
      </p>

      <section>
        <h2>Percentages, never amounts</h2>
        <p>
          The public record contains ratios only. There is no balance, position,
          trade or exchange identifier in it, so someone reading your profile
          cannot tell whether you trade with a hundred dollars or a hundred
          thousand.
        </p>
      </section>

      <section>
        <h2>One combined record</h2>
        <p>
          All your connected accounts are added into a single curve, counted
          from when you connected them. You cannot pick the flattering account
          or hide the weak one. That is what makes the record hard to curate.
          Your one choice is whether the record is public at all, through
          privacy mode.
        </p>
      </section>

      <section>
        <h2>What is measured</h2>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Metric</th>
              <th>Meaning</th>
              <th>Needs</th>
            </tr>
          </thead>
          <tbody>
            {metrics.map(([name, meaning, needs]) => (
              <tr key={name}>
                <td>{name}</td>
                <td>{meaning}</td>
                <td>{needs}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>
          Return is <strong>time-weighted</strong>: the period is split at every
          deposit or withdrawal and the pieces are chained, so adding money
          neither creates nor hides performance. Crypto is valued in a
          stablecoin unit, which is not the same as a dollar, and the unit is
          always stated.
        </p>
        <p>
          When there is not enough history, a metric is shown as{" "}
          <strong>unavailable</strong>. That is different from zero.
        </p>
      </section>

      <section>
        <h2>Proofs and signatures</h2>
        <p>
          Two things can back a statement. Your <strong>signature</strong> shows
          that you authorized exactly that statement. A{" "}
          <strong>zero-knowledge proof</strong> shows that a result was computed
          correctly from data that stays hidden. They answer different
          questions, and Linvesther never presents one as the other.
        </p>
        <p>
          Proofs of performance exist today for individual exchange accounts.
          Proofs for the combined record and for claims are planned. See{" "}
          <Link href="/docs/security">Security and limits</Link>.
        </p>
      </section>

      <section id="origin">
        <h2>Whose data it is</h2>
        <p>
          A proof shows a calculation is right. It cannot show the data it ran
          over was honest, because whoever supplies the data can sign anything.
          So every proof records <strong>which collector</strong> signed the
          data, and a verifier compares that against the collectors it trusts.
        </p>
        <ul>
          <li>
            <strong>Trusted collector.</strong> The signer is on your list, and
            the proof covers a period the collector is trusted for.
          </li>
          <li>
            <strong>Self-attested.</strong> The signer is not on your list. The
            figures are only as trustworthy as whoever holds that key. This is
            what you get from an instance someone runs for themselves.
          </li>
          <li>
            <strong>Revoked or out of date.</strong> The collector was on the
            list but is no longer trusted, or the period is outside the dates it
            was trusted for.
          </li>
        </ul>
        <p>
          Public profiles and claims show which of these applies. The list is
          yours to keep: see <Link href="/docs/verify">Verify a proof</Link>.
        </p>
      </section>

      <section id="verification">
        <h2>Five independent guarantees</h2>
        <p>
          There is no single trust badge. A result is judged on five separate
          questions:
        </p>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Guarantee</th>
              <th>The question</th>
            </tr>
          </thead>
          <tbody>
            {dimensions.map(([name, question]) => (
              <tr key={name}>
                <td>{name}</td>
                <td>{question}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>
          Being strong on one says nothing about another. A perfectly proven
          calculation over false data is still a false result, which is why
          origin and coverage are shown on their own.
        </p>
      </section>
    </>
  );
}
