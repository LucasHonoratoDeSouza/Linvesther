export const metadata = {
  title: "Security and limits",
  description: "What protects you, and what the system does not claim to do.",
};

export default function Security() {
  return (
    <>
      <span className="eyebrow">Concepts</span>
      <h1>Security and limits</h1>
      <p className="document-lead">
        What protects you, and what the system does not claim to do.
      </p>

      <section>
        <h2>What protects you</h2>
        <ul>
          <li>
            <strong>Read-only connections.</strong> Keys that can trade or
            withdraw are rejected. The assets stay at your exchange.
          </li>
          <li>
            <strong>Your key stays with you.</strong> A passkey lives in your
            device&apos;s secure hardware. A password key is encrypted on your
            device. Linvesther never sees either.
          </li>
          <li>
            <strong>Bounded traffic.</strong> Sign-in, connecting an account,
            actions that cost gas and public reads each have a per-client limit,
            the number of worker processes is capped, and a real proof runs one
            at a time.
          </li>
          <li>
            <strong>Nothing leaks in an error.</strong> Failures return a short
            code, never the text of what went wrong, and errors from an exchange
            are stripped of the address that carried your token or signature.
          </li>
          <li>
            <strong>Percentages only.</strong> No balance, position or trade is
            ever published.
          </li>
          <li>
            <strong>Exact consent.</strong> A claim is signed as a whole.
            Nothing can be added or changed after signing.
          </li>
          <li>
            <strong>No operator override.</strong> The registry contracts have
            no upgrade mechanism, and any party can submit a transaction,
            because your signature is what authorizes it.
          </li>
        </ul>
      </section>

      <section>
        <h2>Known limits</h2>
        <ul>
          <li>
            <strong>Data comes from a collector.</strong> Linvesther reads your
            exchange with a read-only key. If the exchange or the collector
            supplied wrong data, a correct calculation gives a wrong result.
          </li>
          <li>
            <strong>Only what you connect.</strong> The record covers connected
            accounts, from the moment of connection. It cannot know about
            accounts you never linked.
          </li>
          <li>
            <strong>Sampled valuation.</strong> Values use prices sampled at a
            fixed cadence. The method is versioned and never changed silently.
          </li>
          <li>
            <strong>Disclosure is permanent.</strong> Once a statement is
            public, copies may exist elsewhere.
          </li>
          <li>
            <strong>No recovery.</strong> If you lose every way of signing in,
            the identity cannot be restored by anyone.
          </li>
          <li>
            <strong>Test network.</strong> The registry runs on Base Sepolia, a
            test network. It carries no economic finality.
          </li>
        </ul>
      </section>

      <section>
        <h2>Where things are heading</h2>
        <ol>
          <li>
            Zero-knowledge proofs for the combined record and for each claim,
            attached on-chain.
          </li>
          <li>
            Stronger origin: authenticated sessions with exchanges and signed
            statements from institutions.
          </li>
          <li>
            More exchanges and brokers, and more metrics with their own
            published methods.
          </li>
          <li>A production deployment on Base after independent review.</li>
        </ol>
      </section>

      <section>
        <h2>Reporting a problem</h2>
        <p>
          Found something wrong? Open an issue on the project&apos;s GitHub
          repository. Please do not post secrets or account data.
        </p>
      </section>
    </>
  );
}
