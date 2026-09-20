import Link from "next/link";

export const metadata = {
  title: "Verifiable trading track records: approaches, limitations and existing systems",
  description:
    "Verifiable trading track records: approaches, limitations and existing systems, from screenshots to zero-knowledge proofs.",
};

export default function Page() {
  return (
    <>
      <span className="eyebrow">Guide</span>
      <h1>Verifiable trading track records: approaches, limitations and existing systems</h1>
      <p className="document-lead">
        There is more than one way to make a performance claim checkable. Each trades off privacy, trust and effort. This page compares them, including where Linvesther&apos;s own approach falls short.
      </p>

      <section>
        <h2>Screenshots and statements</h2>
        <p>
          Simple and universal, and the easiest to fake, crop or curate. They prove nothing on their own and reveal everything they show.
        </p>
      </section>

      <section>
        <h2>Platforms that connect to your account</h2>
        <p>
          A service reads your account through a read-only connection and publishes a verified curve. The reader trusts the platform, and usually sees balances or trades. This is the model most public track-record sites use, and Linvesther starts from the same read-only connection.
        </p>
      </section>

      <section>
        <h2>Data signed by the source</h2>
        <p>
          If the exchange or broker signed the data itself, a verifier could check it without trusting an intermediary. This is the strongest origin, but few venues offer signed statements, so it is rarely available.
        </p>
      </section>

      <section>
        <h2>Trusted hardware</h2>
        <p>
          A trusted execution environment can run the calculation and attest to the result. It hides the data from the operator, but the trust moves to the hardware vendor and its attestation chain.
        </p>
      </section>

      <section>
        <h2>Notarised sessions</h2>
        <p>
          Techniques such as TLSNotary and zero-knowledge TLS let someone prove they received particular data from a website or API over a real TLS session. This can give an origin without the exchange&apos;s cooperation, and is on Linvesther&apos;s roadmap rather than built.
        </p>
      </section>

      <section>
        <h2>Zero-knowledge virtual machines</h2>
        <p>
          A zkVM such as RISC Zero proves that a program ran correctly on private inputs. It answers whether a calculation follows from the data, not whether the data was honest. It is what Linvesther uses for the calculation today.
        </p>
      </section>

      <section>
        <h2>Public registries</h2>
        <p>
          Anchoring identities or checkpoints on a blockchain makes them hard to alter or hide later. Linvesther records identities in a public registry on Base, currently the Sepolia test network.
        </p>
      </section>

      <section>
        <h2>Where Linvesther stands</h2>
        <p>
          Linvesther combines a read-only collector that signs what it reads, zero-knowledge proofs of the calculation, and a public registry. Its honest weak point is the origin: the collector is not the exchange, and anyone can run one, so a proof from an unknown collector is only self-attested. Stronger origins, such as notarised sessions, are the planned way to close that gap.
        </p>
      </section>

      <section>
        <h2>Keep reading</h2>
        <ul>
          <li>
            <Link href="/docs/verifiable-investment-track-record">Verifiable track records</Link>
          </li>
          <li>
            <Link href="/docs/concepts">How it works</Link>
          </li>
          <li>
            <Link href="/docs/security">Security and limits</Link>
          </li>
        </ul>
      </section>
    </>
  );
}
