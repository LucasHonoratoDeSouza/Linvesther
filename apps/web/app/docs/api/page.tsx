import { CodeBlock } from "../../../components/CodeBlock";

export const metadata = {
  title: "Public API",
  description:
    "Read public profiles and claims over HTTP. No account or key is needed, and responses never contain a balance, position, trade or credential.",
};

export default function ApiDocs() {
  return (
    <>
      <span className="eyebrow">Reference</span>
      <h1>Public API</h1>
      <p className="document-lead">
        Read public profiles and claims over HTTP. No account or key is needed,
        and responses never contain a balance, position, trade or credential.
      </p>
      <p>
        Examples use <code>$API_URL</code>, the address of the API you are
        reading from. Every response is JSON. Errors have the shape{" "}
        <code>{`{ "error": "code" }`}</code>.
      </p>

      <section id="profile">
        <h2>Get a public profile</h2>
        <CodeBlock
          label="GET /public/profiles/:address"
          code={'curl "$API_URL/public/profiles/0xYourAddress"'}
        />
        <p>
          Returns the combined record at an address: an optional on-chain{" "}
          <code>profile</code> (name and bio) and the <code>tracks</code> array,
          which holds one entry, all accounts added together.
        </p>
        <CodeBlock
          label="Response"
          code={`{
  "address": "0x…",
  "privacyMode": false,
  "profile": { "name": "…", "bio": "…", "updatedAt": 1789000000 },
  "tracks": [{
    "statement": {
      "trackName": "All accounts",
      "since": "2026-09-18T12:00:00.000Z",
      "trackedDays": 12,
      "metrics": {
        "return": "0.0421",
        "maxDrawdown": "0.0210",
        "sharpe": "1.32",
        "winRate": { "value": "0.58", "closedTrades": 24 }
      },
      "computedAt": "2026-09-30T12:00:00.000Z"
    },
    "unavailable": []
  }],
  "origin": { "mechanism": "A0", "collector": "9c177b47…c134" }
}`}
        />
        <p>
          Ratios are decimal strings, so <code>"0.0421"</code> is 4.21%. A
          metric that cannot be computed yet is missing from{" "}
          <code>metrics</code> and listed in <code>unavailable</code>. In
          privacy mode the response is{" "}
          <code>{`{ "privacyMode": true, "profile": null, "tracks": [] }`}</code>
          .
        </p>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Status</th>
              <th>Meaning</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>400 not_an_address</td>
              <td>The path is not a 0x address.</td>
            </tr>
            <tr>
              <td>404 no_public_profile</td>
              <td>Nothing is public at that address.</td>
            </tr>
            <tr>
              <td>502 profile_unavailable</td>
              <td>The data source could not be reached. Retry.</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section id="series">
        <h2>Get the return curve</h2>
        <CodeBlock
          label="GET /public/profiles/:address/series?range=30d"
          code={
            'curl "$API_URL/public/profiles/0xYourAddress/series?range=30d"'
          }
        />
        <p>
          <code>range</code> is one of <code>24h</code>, <code>7d</code>,{" "}
          <code>30d</code>, <code>1y</code>, <code>5y</code> or <code>max</code>{" "}
          (the default). The response has <code>points</code>, each with{" "}
          <code>timeMs</code> and <code>returnFraction</code> (return since the
          start of the range), plus <code>stepMs</code> and <code>sinceMs</code>
          . Errors: 400 for a bad address or range, 404 when nothing is public.
        </p>
      </section>

      <section id="collector">
        <h2>Get the collector</h2>
        <CodeBlock
          label="GET /public/collector"
          code={'curl "$API_URL/public/collector"'}
        />
        <p>
          Returns{" "}
          <code>{`{ "mechanism": "A0", "collector": "<fingerprint>" }`}</code>:
          the key that signs the figures this instance reads from exchanges.{" "}
          <code>collector</code> is <code>null</code> when no signing key is
          configured. Profiles and claims carry the same <code>origin</code>.
          Compare the fingerprint with a list of collectors you trust; one that
          is not on your list is self-attested. See{" "}
          <a href="/docs/verify#origin">Verify a proof</a>.
        </p>
      </section>

      <section id="claims">
        <h2>Get a claim</h2>
        <CodeBlock
          label="GET /public/claims/:digest"
          code={'curl "$API_URL/public/claims/$DIGEST"'}
        />
        <p>
          The digest is the identifier in a claim link (
          <code>/c/&lt;digest&gt;</code>).
        </p>
        <CodeBlock
          label="Response"
          code={`{
  "digest": "7a34269267a8…",
  "owner": "0x…",
  "claimSet": {
    "identityId": "0x…",
    "trackId": "all",
    "periodStart": "2026-09-18T12:00:00.000Z",
    "periodEnd": "2026-09-30T12:00:00.000Z",
    "claims": [{ "type": "LE", "metric": "maxDrawdown", "threshold": "0.03" }],
    "audience": "public",
    "expiresAt": "2026-10-30T12:00:00.000Z"
  },
  "publishedAt": "2026-09-30T12:00:00.000Z",
  "verification": "owner_signed_collector_attested",
  "origin": { "mechanism": "A0", "collector": "9c177b47…c134" }
}`}
        />
        <p>
          Predicate types are <code>GE</code> (at least) and <code>LE</code> (at
          most). The response never includes the figure behind the statement.
          Errors: 404 when it does not exist or is not public, 410 when it has
          expired.
        </p>
        <p>
          The digest is the SHA-256 of the claim set in canonical JSON (RFC
          8785), which is what the owner signed.
        </p>
      </section>

      <section id="account">
        <h2>Read your own account</h2>
        <p>
          These are not part of the public API above: each one needs a
          read-only token for the account it reads, created at{" "}
          <code>/settings/api-tokens</code>, and answers with your own
          balances, positions and trades rather than another address&apos;s
          public figures.
        </p>
        <CodeBlock
          label="GET /mcp/account/nav"
          code={'curl -H "Authorization: Bearer $TOKEN" "$API_URL/mcp/account/nav"'}
        />
        <p>
          The six routes are <code>/mcp/account/state</code>,{" "}
          <code>portfolio</code>, <code>nav</code>, <code>performance</code>,{" "}
          <code>trades</code> and <code>series</code>, one read-only route per
          figure the account pages show you. They are also reachable as MCP
          tools, for an AI agent to call directly — see{" "}
          <a href="/docs/mcp">MCP server</a> for the token format, the tool
          list and the guarantees that keep them read-only.
        </p>
      </section>

      <section id="cors">
        <h2>Notes</h2>
        <ul>
          <li>
            Public reads are cached for up to a minute; each profile says when
            it was computed.
          </li>
          <li>
            Requests from a browser must come from an origin the API allows.
            Server-side use has no such limit.
          </li>
          <li>
            Ownership and registration can also be read straight from the chain.
            See Contracts.
          </li>
        </ul>
      </section>
    </>
  );
}
