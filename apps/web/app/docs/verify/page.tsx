import { CodeBlock } from "../../../components/CodeBlock";

export const metadata = {
  title: "Verify a proof",
  description:
    "Check a proof yourself, offline, without trusting Linvesther or anyone else's server.",
};

export default function VerifyDocs() {
  return (
    <>
      <span className="eyebrow">Reference</span>
      <h1>Verify a proof</h1>
      <p className="document-lead">
        Check a proof yourself, offline, without trusting Linvesther or anyone
        else&apos;s server.
      </p>

      <section>
        <h2>What you need</h2>
        <ul>
          <li>
            A <strong>bundle</strong>: the folder an owner exports, with the
            proof, the account set and the manifest.
          </li>
          <li>
            The <code>linvesther-verify</code> command-line tool, built from{" "}
            <code>crates/verifier</code>.
          </li>
          <li>
            Optionally, a <strong>list of collectors you trust</strong>. Without
            it, no collector is trusted and every proof reads as self-attested.
          </li>
        </ul>
      </section>

      <section>
        <h2>Run it</h2>
        <CodeBlock
          label="Shell"
          code={`cargo run -p verifier --bin linvesther-verify -- \\
  ./bundle \\
  --trusted-collectors trust/collectors.json`}
        />
        <p>
          The tool prints one result per check and two summary lines: whether
          the bundle is valid as of now, and where its data comes from.
        </p>
        <CodeBlock
          label="Output"
          code={`summary: VALID_AS_OF(1789000000000)
origin: ORIGIN=TRUSTED_COLLECTOR(example collector)`}
        />
      </section>

      <section>
        <h2>The checks</h2>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Check</th>
              <th>Fails when</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Hashes</td>
              <td>A file was changed or is missing.</td>
            </tr>
            <tr>
              <td>Account set</td>
              <td>The listed accounts do not produce the committed root.</td>
            </tr>
            <tr>
              <td>Calculation</td>
              <td>
                The receipt is not a real proof for the released program, or its
                journal was swapped.
              </td>
            </tr>
            <tr>
              <td>Registry</td>
              <td>
                The claimed on-chain anchor is not one you obtained
                independently.
              </td>
            </tr>
            <tr>
              <td>Coverage</td>
              <td>
                The gaps the bundle claims differ from the ones the calendar
                implies.
              </td>
            </tr>
          </tbody>
        </table>
        <p>
          Offline, a pass is always reported as <code>VALID_AS_OF</code> a
          moment, never as current: the tool cannot see corrections published
          later.
        </p>
      </section>

      <section id="origin">
        <h2>Where the data comes from</h2>
        <p>
          The origin line is separate from the checks above, because they answer
          different questions. It is one of:
        </p>
        <table className="dimension-table">
          <tbody>
            <tr>
              <td>TRUSTED_COLLECTOR(name)</td>
              <td>
                Signed by a collector on your list, for a period it is trusted
                for.
              </td>
            </tr>
            <tr>
              <td>SELF_ATTESTED</td>
              <td>Signed by a key that is not on your list.</td>
            </tr>
            <tr>
              <td>REVOKED_COLLECTOR(name)</td>
              <td>The collector is on your list but marked revoked.</td>
            </tr>
            <tr>
              <td>COLLECTOR_OUTSIDE_VALIDITY(name)</td>
              <td>
                The proof covers a period outside the collector&apos;s dates.
              </td>
            </tr>
            <tr>
              <td>ORIGIN_UNAVAILABLE</td>
              <td>There was no verified receipt to read the signer from.</td>
            </tr>
          </tbody>
        </table>
        <p>
          A valid calculation over data you do not trust is not a verified track
          record, so the tool exits with status 3 in that case. Pass{" "}
          <code>--accept-self-attested</code> if that is what you want, for
          example to check your own instance. The origin is still printed.
        </p>
        <p>
          Exit status: <code>0</code> valid and trusted, <code>1</code> a check
          failed, <code>3</code> valid but the origin is not trusted.
        </p>
      </section>

      <section id="trust-list">
        <h2>The trust list</h2>
        <p>
          The list is a JSON file you control. The repository ships{" "}
          <code>trust/collectors.json</code>, which starts empty; a collector is
          added only deliberately.
        </p>
        <CodeBlock
          label="trust/collectors.json"
          code={`{
  "version": 1,
  "collectors": [
    {
      "name": "example collector",
      "fingerprint": "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134",
      "validFromMs": 1780000000000,
      "validUntilMs": 1810000000000,
      "revoked": false
    }
  ]
}`}
        />
        <ul>
          <li>
            <code>fingerprint</code> is 64 hex characters: SHA-256 over a fixed
            tag and the collector&apos;s compressed public key. An instance
            reports its own at <code>/public/collector</code>.
          </li>
          <li>
            <code>validFromMs</code> and <code>validUntilMs</code> are optional.
            If set, the proof&apos;s period must fall inside them.
          </li>
          <li>
            A collector marked <code>revoked</code> is never trusted, at any
            date: from a proof alone there is no way to show a signature
            predates a key compromise.
          </li>
          <li>
            A malformed list is refused. It is never read as an empty one.
          </li>
        </ul>
        <div className="document-notice">
          Trust is your decision. The list on a website is a convenience: to be
          sure, keep your own and verify with it.
        </div>
      </section>
    </>
  );
}
