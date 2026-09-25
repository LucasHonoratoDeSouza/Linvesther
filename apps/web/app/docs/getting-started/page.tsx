import Link from "next/link";

export const metadata = {
  title: "Getting started",
  description: "From nothing to a shareable claim in about five minutes.",
};

export default function GettingStarted() {
  return (
    <>
      <span className="eyebrow">Start</span>
      <h1>Getting started</h1>
      <p className="document-lead">
        From nothing to a shareable claim in about five minutes.
      </p>

      <section>
        <h2>1. Create your identity</h2>
        <p>
          Open <Link href="/onboarding">Your identity</Link> and choose how you
          will sign in:
        </p>
        <ul>
          <li>
            <strong>Your device.</strong> A fingerprint, face or screen lock,
            through a passkey. Nothing to remember.
          </li>
          <li>
            <strong>A password.</strong> A key is created in your browser and
            stored encrypted on your device. Download the backup file it offers
            and keep it somewhere safe.
          </li>
        </ul>
        <p>
          Linvesther never sees your password or your key. If you lose every way
          of signing in, no one can restore the identity, including us.
        </p>
      </section>

      <section>
        <h2>2. Connect an account</h2>
        <p>
          In <Link href="/portfolio">Portfolio</Link>, choose Add account and
          follow the steps for your exchange or broker. Credentials must be
          read-only; ones that can trade or move money are rejected.
          Step-by-step guides are in{" "}
          <Link href="/docs/connections">Connect an account</Link>.
        </p>
      </section>

      <section>
        <h2>3. Look at your performance</h2>
        <p>
          The first figures appear right away. Some, such as Sharpe ratio, need
          at least 30 days of history and show as unavailable until then. That
          is intentional: a missing number is better than a made-up one.
        </p>
        <p>
          All your connected accounts are combined, and results are counted from
          the moment you connected each one.
        </p>
      </section>

      <section>
        <h2>4. Choose what is public</h2>
        <p>
          <Link href="/disclose">Public profile</Link> shows what anyone with
          your address sees: percentages only. You can add an optional name and
          bio in Portfolio, and turn on <strong>Full privacy mode</strong> to
          hide everything.
        </p>
      </section>

      <section>
        <h2>5. Share a claim</h2>
        <p>
          In <Link href="/claims">Claims</Link>, pick a statement, such as
          “return at least 10%”, and authorize it with your password or device.
          You get a link that shows only that statement. See{" "}
          <Link href="/docs/claims">Claims</Link>.
        </p>
      </section>
    </>
  );
}
