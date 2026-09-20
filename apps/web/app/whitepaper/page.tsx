import Link from "next/link";
import { Icon } from "../../components/Icon";
import { SiteFooter } from "../../components/SiteFooter";

export const metadata = { title: "Whitepaper" };

const PDF = "/linvesther-whitepaper.pdf";

const contents = [
  [
    "The problem",
    "Why a track record forces a choice between an unbacked number and total exposure.",
  ],
  [
    "Identity and ownership",
    "Passkeys, password-derived keys and smart accounts on Base.",
  ],
  [
    "Measuring performance",
    "Read-only sources, time-weighted return, drawdown and combined accounts.",
  ],
  ["Selective disclosure", "Claims that reveal a threshold and nothing more."],
  [
    "Zero-knowledge proofs",
    "Showing a result is correct without showing the inputs.",
  ],
  [
    "Trust model and limits",
    "Five independent guarantees, and what the system does not defend against.",
  ],
];

export default function WhitepaperPage() {
  return (
    <>
      <main className="container paper-page">
        <span className="eyebrow">Whitepaper / v0.1 draft</span>
        <h1>
          Verifiable performance.
          <br />
          <span>Private history.</span>
        </h1>
        <p className="document-lead">
          A protocol for turning a trader&apos;s real track record into public
          statements that anyone can check, without publishing a balance, a
          position or a trade.
        </p>
        <div className="paper-actions">
          <a className="button button-accent" href={PDF} download>
            Download the PDF <Icon name="diagonal" width={17} />
          </a>
          <Link className="button button-outline" href="/docs">
            Read the docs <Icon name="arrow" width={17} />
          </Link>
        </div>

        <section className="paper-section">
          <h2>Abstract</h2>
          <p>
            Financial performance is easy to claim and hard to verify. A
            screenshot can be edited and a spreadsheet can be curated; the only
            way to prove more is to hand over the account itself. Linvesther
            measures performance from read-only connections, expresses it only
            as ratios, and publishes narrow statements the owner authorizes one
            by one. Where a stronger guarantee is available, the calculation is
            proven with a zero-knowledge proof. Ownership and registration are
            anchored on a public blockchain.
          </p>
          <p>
            The paper describes the technology and the guarantees it provides,
            and is explicit about the ones it does not.
          </p>
        </section>

        <section className="paper-section">
          <h2>Inside</h2>
          <ol className="paper-contents">
            {contents.map(([title, text], index) => (
              <li key={title}>
                <span>{String(index + 1).padStart(2, "0")}</span>
                <div>
                  <strong>{title}</strong>
                  <p>{text}</p>
                </div>
              </li>
            ))}
          </ol>
        </section>

        <div className="document-notice">
          This is a draft for public review. It describes the technology, not
          the implementation, and it marks what is planned as planned. The
          registry currently runs on the Base Sepolia test network.
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
