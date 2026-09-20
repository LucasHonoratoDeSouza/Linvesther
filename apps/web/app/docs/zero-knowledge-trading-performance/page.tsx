import Link from "next/link";

export const metadata = {
  title: "Zero-knowledge trading performance",
  description:
    "How a zero-knowledge virtual machine can prove a trading result follows from private data without revealing that data.",
};

export default function Page() {
  return (
    <>
      <span className="eyebrow">Guide</span>
      <h1>Zero-knowledge trading performance</h1>
      <p className="document-lead">
        How a trading result can be proven correct while the trades behind it stay private.
      </p>

      <section>
        <h2>The idea</h2>
        <p>
          A zero-knowledge proof lets someone show that a computation was done correctly on inputs the verifier never sees. For performance, the computation is the metric calculation and the private inputs are the trading history.
        </p>
      </section>

      <section>
        <h2>How Linvesther does it</h2>
        <p>
          The metric calculation is an ordinary program that runs inside the RISC Zero zkVM. Running it produces a receipt that binds three things: the identifier of the exact program that ran, a public journal (for example the period and the result) and a proof that the program executed correctly. The private inputs appear only as commitments, so the receipt reveals the result and nothing about the trades.
        </p>
        <p>
          A verifier needs the receipt and an independent source for the accepted program identifier. With those, they can check the proof offline, without trusting Linvesther&apos;s servers.
        </p>
      </section>

      <section>
        <h2>Where the data comes from</h2>
        <p>
          The journal also records the fingerprint of the collector key that signed the source data. A proof shows that a calculation is right, not that the data was honest, so a verifier compares that fingerprint with a list of collectors they trust. A signer that is not on the list is reported as self-attested.
        </p>
      </section>

      <section>
        <h2>Current scope</h2>
        <p>
          Proofs exist today for each exchange account, covering return and maximum drawdown. Proofs for the combined record and for claims are planned. Generating a proof is memory-hungry, which is why it runs on demand rather than continuously.
        </p>
      </section>

      <section>
        <h2>Keep reading</h2>
        <ul>
          <li>
            <Link href="/docs/verify">Verify a proof</Link>
          </li>
          <li>
            <Link href="/docs/prove-performance-without-revealing-trades">Prove it without revealing trades</Link>
          </li>
          <li>
            <Link href="/whitepaper">Whitepaper</Link>
          </li>
        </ul>
      </section>
    </>
  );
}
