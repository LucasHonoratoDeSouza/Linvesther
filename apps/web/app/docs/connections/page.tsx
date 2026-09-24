export const metadata = { title: "Connect an account" };

export default function Connections() {
  return (
    <>
      <span className="eyebrow">Start</span>
      <h1>Connect an account</h1>
      <p className="document-lead">
        Every connection is read-only. Linvesther reads your history to compute
        percentages; it cannot trade, withdraw or move anything.
      </p>
      <div className="document-notice">
        Credentials that carry trading, transfer or withdrawal permissions are
        rejected when you connect. Create a dedicated read-only key for
        Linvesther and delete it whenever you like.
      </div>

      <section>
        <h2>Binance</h2>
        <ol>
          <li>
            In Binance, open Account, then API Management, then Create API.
          </li>
          <li>
            Turn on <strong>Enable Reading</strong> only.
          </li>
          <li>Copy the API key and the secret into Linvesther.</li>
        </ol>
        <p>
          Spot accounts are supported. Updates arrive within moments. Money you
          put in Simple Earn (flexible or locked, which is where most staking
          sits) still counts as yours: moving it between spot and Earn changes
          nothing, and what Earn pays is counted as performance, not as money
          you added. Other products, such as futures, margin or the Funding
          wallet, are not read.
        </p>
      </section>

      <section>
        <h2>Coinbase</h2>
        <ol>
          <li>
            Open the Coinbase Developer Platform (
            <code>portal.cdp.coinbase.com</code>), then API keys, then Create
            API key.
          </li>
          <li>
            Give it the <strong>View</strong> permission only. Ed25519 and ECDSA
            keys both work.
          </li>
          <li>
            Copy the key ID (or name) and the secret. Coinbase shows the secret
            once.
          </li>
        </ol>
        <p>
          Advanced Trade accounts are supported. Updates arrive within moments.
        </p>
      </section>

      <section>
        <h2>Kraken</h2>
        <ol>
          <li>
            In Kraken, open Security, then API (or Settings, then API), and
            create a new key.
          </li>
          <li>
            Tick only these four, which sit in different groups:{" "}
            <strong>Query funds</strong> (under Funds),{" "}
            <strong>Query open orders &amp; trades</strong> and{" "}
            <strong>Query closed orders &amp; trades</strong> (under Orders and
            trades), and <strong>Query ledger entries</strong> (under Data,
            easy to miss). Leave order, deposit and withdrawal permissions off.
          </li>
          <li>
            Copy the API key and the private key. Kraken shows the private key
            once.
          </li>
        </ol>
        <p>
          Spot accounts are supported. Kraken cannot tell an application what a
          key may do, so Linvesther checks it by trying, without changing
          anything: a key that Kraken allows to place an order or read
          withdrawal methods is refused. Deposits and withdrawals are read from your
          Kraken ledger, and staking rewards count as performance, not as money
          you put in. Kraken keeps price history only for a limited time, so a
          very old range may not be drawn at the finest detail.
        </p>
      </section>

      <section>
        <h2>On-chain wallet</h2>
        <ol>
          <li>Choose Crypto wallet, then pick the wallet installed in your browser and the address to follow.</li>
          <li>
            Sign the one message it shows. It only proves the address is yours: it is
            not a transaction, costs nothing and gives no access to your funds. Linvesther
            never asks a wallet for anything else.
          </li>
        </ol>
        <p>
          Your address stays private: it is stored encrypted, shown only to you, and
          never appears on your public profile, in a claim or in a proof. The public
          curve of a wallet has at most one point a day, because a finer one could be
          matched against the address&apos;s public history on its chain. Balances and
          transfers are read across Ethereum, Base, Arbitrum, Optimism and Polygon
          (BNB Chain and Avalanche can be enabled by the operator). Money you send in or out counts as a deposit or
          withdrawal; swaps and gas count as performance. A token with no price makes
          the metrics unavailable instead of quietly dropping it.
        </p>
      </section>

      <section>
        <h2>Interactive Brokers</h2>
        <ol>
          <li>
            In Account Management, go to Settings, then Reporting, then Flex
            Queries. Create an <strong>Activity Flex Query</strong> with the
            Cash Transactions, Trades and Equity Summary sections, a daily
            period and XML output.
          </li>
          <li>
            Under Flex Web Service, generate a token. It is read-only. If you
            restrict it by IP address, the report may be refused, so leave that
            open or allow the server address.
          </li>
          <li>Copy the Flex Query ID and the token into Linvesther.</li>
        </ol>
        <p>
          Interactive Brokers reports once a day, so figures update daily rather
          than continuously.
        </p>
      </section>

      <section>
        <h2>Several accounts</h2>
        <p>
          You can connect any combination. They are added together into one
          public record, and you cannot leave one out. Moving money between your
          own accounts does not count as a gain. Give each account a name with
          the pencil icon in Portfolio.
        </p>
      </section>

      <section id="remove">
        <h2>Removing an account</h2>
        <p>
          In Portfolio, hover an account and use the bin icon. Removing an account deletes the stored key and everything
          collected for it, and it stops counting in your public profile. Revoke the key at the exchange as well if you
          no longer want it to exist there.
        </p>
      </section>

      <section>
        <h2>If a connection fails</h2>
        <ul>
          <li>
            <strong>Refused because of permissions.</strong> The key can trade
            or withdraw. Create a new key with reading only.
          </li>
          <li>
            <strong>Access denied.</strong> The key or token is wrong, expired
            or restricted by IP address.
          </li>
          <li>
            <strong>Not enough history.</strong> Some figures appear only after
            enough days. Connect, then check back.
          </li>
        </ul>
      </section>
    </>
  );
}
