import { CodeBlock } from "../../../components/CodeBlock";

export const metadata = { title: "Run it yourself" };

const persisted = [
  [
    "Sign-ins",
    "Postgres, for 30 days",
    "Signed out when the session expires or you sign out.",
  ],
  [
    "Identities and passkeys",
    "Postgres",
    "Without them, nobody could sign in again after a restart.",
  ],
  [
    "Published claims",
    "Postgres",
    "A link someone shared keeps working until the claim expires.",
  ],
  ["Profile settings", "Postgres", "Privacy mode never switches itself off."],
  [
    "Connected accounts and history",
    "The worker's Postgres tables",
    "Credentials are stored encrypted.",
  ],
  [
    "Ownership and registration",
    "The chain",
    "Public, and rebuilt from events if the database is lost.",
  ],
];

export default function SelfHosting() {
  return (
    <>
      <span className="eyebrow">Reference</span>
      <h1>Run it yourself</h1>
      <p className="document-lead">
        Run the whole stack on your own machine, with no outside network: a
        local database, a local chain and the app.
      </p>

      <section>
        <h2>Start a local environment</h2>
        <p>
          You need Docker, <a href="https://getfoundry.sh">Foundry</a>, Node.js
          24 with pnpm, and OpenSSL.
        </p>
        <CodeBlock label="Shell" code={`infra/local/up.sh`} />
        <p>The script:</p>
        <ul>
          <li>
            starts the project&apos;s Postgres container and creates a database
            called <code>linvesther_local</code>, separate from any other
            database on that server;
          </li>
          <li>
            starts a local chain (Anvil) and deploys the registry contracts to
            it. The chain&apos;s state is saved in{" "}
            <code>infra/local/.state</code>, so it survives a restart;
          </li>
          <li>
            writes <code>.env.local</code> with the addresses, fresh secrets and
            a signing key for your instance. It is never committed.
          </li>
        </ul>
        <p>
          Then start the API and the web app in two terminals, and open the
          address it prints.
        </p>
        <CodeBlock
          label="Shell"
          code={`set -a; source .env.local; set +a; pnpm --filter @linvestherzk/api start
pnpm --filter @linvestherzk/web dev`}
        />
        <p>
          Build the worker once with{" "}
          <code>
            cargo build -p binance-worker --manifest-path services/Cargo.toml
          </code>
          . Add <code>--no-default-features</code> if you do not need
          zero-knowledge proofs, which require the RISC Zero toolchain.
        </p>
        <p>
          <code>infra/local/down.sh</code> stops the chain, and{" "}
          <code>infra/local/up.sh --reset</code> forgets the local chain and
          database and starts over.
        </p>
      </section>

      <section id="persistence">
        <h2>What is kept</h2>
        <table className="dimension-table">
          <thead>
            <tr>
              <th>State</th>
              <th>Where</th>
              <th>Note</th>
            </tr>
          </thead>
          <tbody>
            {persisted.map(([state, where, note]) => (
              <tr key={state}>
                <td>{state}</td>
                <td>{where}</td>
                <td>{note}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p>
          All of this needs <code>DATABASE_URL</code>. Without it the API starts
          anyway, warns, and keeps sign-ins, identities and claims in memory, so
          they are lost when it restarts. Sign-in challenges, traffic limits and the gas
          budget are shared in Postgres too, so several API instances behave as one; without a database they are per
          process, so run a single instance.
        </p>
      </section>

      <section id="trust">
        <h2>Your instance and trust</h2>
        <p>
          Your instance signs the figures it reads with its own key, so anything
          it proves is <strong>self-attested</strong> for everyone else: nobody
          has a reason to trust a key they have not chosen to trust. That is by
          design. It makes your own numbers fully usable to you, and it keeps a
          self-run instance from passing for anyone else&apos;s.
        </p>
        <p>
          Your instance reports its collector at <code>/public/collector</code>.
          Someone who trusts you can add that fingerprint to their own list. See{" "}
          <a href="/docs/verify#trust-list">Verify a proof</a>.
        </p>
      </section>

      <section>
        <h2>Beyond your machine</h2>
        <ul>
          <li>
            Passkeys need HTTPS on a real domain. Set{" "}
            <code>WEBAUTHN_RP_ID</code> and <code>WEBAUTHN_ORIGIN</code> to it;
            the session cookie is then marked <code>Secure</code>.
          </li>
          <li>
            Set <code>CORS_ORIGINS</code> to the origins of your web app. A
            state-changing request from any other origin is refused.
          </li>
          <li>
            Point <code>CHAIN_RPC_URL</code> and the contract addresses at the
            network you deploy to, and set <code>CHAIN_DEPLOY_BLOCK</code> so
            the indexer does not scan from the start of the chain.
          </li>
          <li>
            Behind a reverse proxy or tunnel, set <code>TRUST_PROXY_HOPS</code>{" "}
            to the number of proxies in front of the API (usually <code>1</code>
            ). Rate limits are per client address, and without this every client
            looks like the proxy and shares one allowance. Leave it unset when
            the API is reachable directly.
          </li>
          <li>
            Keep the database on a private network. The default password in the
            repository is public; the local environment binds Postgres to this
            machine only, and yours should be reachable only by the API and
            worker.
          </li>
          <li>
            Serve the web app with <code>next build</code> and{" "}
            <code>next start</code>, not the development server.
          </li>
          <li>
            Set your own <code>POSTGRES_PASSWORD</code>. Secrets can also be
            mounted as files (<code>BINANCE_WORKER_ENCRYPTION_KEY_FILE</code>,{" "}
            <code>BINANCE_WORKER_A0_SIGNING_KEY_FILE</code>, mode 600), which
            keeps them out of the process environment.{" "}
            <code>RELAY_MAX_PER_HOUR</code> caps the gas you will fund.
          </li>
          <li>
            Keep <code>BINANCE_WORKER_ENCRYPTION_KEY</code> and{" "}
            <code>BINANCE_WORKER_A0_SIGNING_KEY</code> safe. The first protects
            stored credentials; losing the second changes your collector
            identity.
          </li>
        </ul>
      </section>
    </>
  );
}
