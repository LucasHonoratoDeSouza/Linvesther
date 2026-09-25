import Link from "next/link";

export const metadata = {
  title: "Claims",
  description:
    "A claim is one statement about your track record, shared as a link. It says a threshold is met and nothing about the exact figure.",
};

export default function ClaimsDocs() {
  return (
    <>
      <span className="eyebrow">Concepts</span>
      <h1>Claims</h1>
      <p className="document-lead">
        A claim is one statement about your record, shared as a link. It says a
        threshold is met and nothing about the exact figure.
      </p>

      <section>
        <h2>Why claims exist</h2>
        <p>
          Your public profile shows every percentage. Sometimes you would rather
          show only one fact, such as “drawdown never worse than 10%”. A claim
          does that. Pair claims with <strong>Full privacy mode</strong> to keep
          the rest of your record hidden.
        </p>
      </section>

      <section>
        <h2>Creating a claim</h2>
        <ol>
          <li>
            Open <Link href="/claims">Claims</Link> and choose a statement:{" "}
            <strong>return at least</strong> or{" "}
            <strong>max drawdown at most</strong>.
          </li>
          <li>Enter the percentage.</li>
          <li>
            Authorize it with your password or device, then copy the link.
          </li>
        </ol>
        <p>
          The claim covers your combined record. It is checked against your real
          figures first, so a statement that is not true is rejected. A claim is
          valid for 30 days.
        </p>
      </section>

      <section>
        <h2>What the link shows</h2>
        <p>
          The statement, the address that made it, the period it covers and when
          it expires. It does not show the figure behind it. The address is
          included so the claim can be attributed and its signature checked. If
          your profile is public, that address also leads to your full profile,
          so turn on privacy mode if you only want to share the claim.
        </p>
      </section>

      <section>
        <h2>What a claim guarantees</h2>
        <ul>
          <li>
            <strong>You authorized it.</strong> The signature covers the exact
            statement, period, audience and expiry. Changing any of them
            invalidates it.
          </li>
          <li>
            <strong>It was true when signed.</strong> It was checked against
            figures read from your accounts.
          </li>
        </ul>
        <div className="document-notice">
          A claim is not yet a zero-knowledge proof. The figures come from a
          read-only connection, checked by Linvesther at signing time. Proofs
          for claims are on the roadmap.
        </div>
      </section>

      <section>
        <h2>Reading a claim over HTTP</h2>
        <p>
          Claims are readable without signing in. See{" "}
          <Link href="/docs/api">Public API</Link>.
        </p>
      </section>
    </>
  );
}
