import { CodeBlock } from "../../../components/CodeBlock";

export const metadata = { title: "MCP server" };

const tools = [
  ["get_account_state", "Every connected account, each connection's health, each one's current value and their total."],
  ["get_portfolio", "Positions held, their average cost, and profit since the account was connected — realized, unrealized and fees."],
  ["get_nav", "What the account is worth right now, from its real balance and real market prices, and which assets make it up."],
  ["get_performance", "CAGR, Sharpe, Sortino, maximum drawdown and win rate."],
  ["get_trades", "The trades already collected for this account, filtered by market and period, paged."],
  ["get_chart_series", "The account's value and time-weighted return over a range, as points for a chart."],
] as const;

export default function McpDocs() {
  return (
    <>
      <span className="eyebrow">Reference</span>
      <h1>MCP server</h1>
      <p className="document-lead">
        Connect an AI agent to your own account with the Model Context
        Protocol. It can read what you can see — connected accounts, value,
        positions, performance, trades, the return curve — and nothing else.
        There is no tool that connects an account, places an order, moves
        funds or changes a setting.
      </p>

      <section id="token">
        <h2>Get a token</h2>
        <p>
          Create one at <code>/settings/api-tokens</code>. It looks like{" "}
          <code>lvz_ro_…</code>, is shown once, and only its hash is kept
          after that — if you lose it, revoke it and create another. You can
          hold up to 20 at a time, and revoking one takes effect immediately.
        </p>
        <p>
          A token reads only the identity that created it. Give it to an
          agent the same way you would give it a password: as an environment
          variable or a client&apos;s own secret store, never pasted into a
          prompt or a shared file.
        </p>
      </section>

      <section id="connect">
        <h2>Connect a client</h2>
        <p>
          The production server is at{" "}
          <code>https://mcp.linvesther.com/mcp</code>, standard MCP over
          Streamable HTTP. If you run your own instance, point at its address
          instead — see <a href="/docs/self-hosting">Run it yourself</a>.
        </p>
        <CodeBlock
          label="Shell"
          code={`claude mcp add --transport http linvesther https://mcp.linvesther.com/mcp \\
  --header "Authorization: Bearer $TOKEN"`}
        />
        <p>
          Any MCP client that speaks Streamable HTTP works the same way: send
          the token as <code>Authorization: Bearer lvz_ro_…</code> on every
          call. There is no configured fallback token, so a call sent without
          one is refused rather than answered with somebody else&apos;s
          account.
        </p>
      </section>

      <section id="tools">
        <h2>The six tools</h2>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>Tool</th>
              <th>Returns</th>
            </tr>
          </thead>
          <tbody>
            {tools.map(([name, description]) => (
              <tr key={name}>
                <td>
                  <code>{name}</code>
                </td>
                <td>{description}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>
          Each tool publishes a JSON Schema for its input and its output, so
          a client can validate both without reading prose. A figure with too
          little history to compute comes back <code>null</code> with its own
          reason, never a zero standing in for it. Amounts cross as decimal
          strings, since a double cannot hold every value a balance can take.
        </p>
      </section>

      <section id="security">
        <h2>What keeps this read-only</h2>
        <ul>
          <li>
            <strong>Nothing that writes is installed alongside it.</strong>{" "}
            The server ships as its own package, with no dependency on the
            code that connects an exchange, renames or removes an account, or
            starts a proof. There is no disabled write path to re-enable —
            the code simply is not there.
          </li>
          <li>
            <strong>One credential, one purpose.</strong> A read-only token
            is checked by different code than your browser session, and
            neither can stand in for the other. It authorizes exactly the six
            reads above, is limited to 60 requests a minute per token, and
            stops working the moment you revoke it.
          </li>
          <li>
            <strong>No silent fallback.</strong> Every call must carry its
            own token. A request sent without one is refused, never answered
            using some other configured identity.
          </li>
          <li>
            <strong>Runs on its own.</strong> Each call gets its own server
            instance, closed when the call ends, so nothing from one
            identity&apos;s request can leak into another&apos;s. The
            listening address only accepts requests whose <code>Host</code>{" "}
            and <code>Origin</code> match an allowed hostname, which defends
            self-hosted instances against a browser page redirected at
            <code> localhost</code>.
          </li>
        </ul>
      </section>

      <section id="api">
        <h2>Without MCP</h2>
        <p>
          The same six reads are also plain HTTP endpoints under{" "}
          <code>/mcp/account/</code>, authenticated the same way — see{" "}
          <a href="/docs/api#account">Public API</a> if you would rather call
          them directly than through an MCP client.
        </p>
      </section>
    </>
  );
}
