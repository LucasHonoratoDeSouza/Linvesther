import { connectBinance, connectCoinbase, connectIbkr, connectKraken, connectWallet, type ApiResult } from "../../lib/api";

/** The one list of places an account can be connected from. The "Add
 * account" popup, the connect form and every account row are all driven
 * by this — adding a broker later (another crypto exchange, a stock
 * broker) means adding an entry here plus the API route it connects
 * through, not touching the screens. */
export interface BrokerDefinition {
  id: string;
  name: string;
  kind: "crypto" | "stocks" | "onchain";
  /** The broker's own logo (a file under public/), so people recognise it at a glance. */
  logo: string;
  /** Rounds the logo's own corners — for a square asset that isn't already a circle/icon-shaped mark. */
  roundedLogo?: boolean;
  /** One line shown under the name in the picker. */
  summary: string;
  /** Plain steps for getting a read-only credential from that broker. */
  instructions: string[];
  /** What the person pastes in. `secret` fields are hidden as typed. */
  fields: { key: string; label: string; secret: boolean; multiline?: boolean }[];
  /** "wallet": the person connects by choosing a wallet and signing a message, not by pasting credentials. */
  method?: "credentials" | "wallet";
  connect: (accountId: string, values: Record<string, string>, label?: string) => Promise<ApiResult<unknown>>;
}

export const BROKER_KINDS: { kind: BrokerDefinition["kind"]; title: string; empty: string }[] = [
  { kind: "crypto", title: "Crypto exchanges", empty: "More exchanges are coming." },
  { kind: "stocks", title: "Stock brokers", empty: "More brokers are coming." },
  { kind: "onchain", title: "On-chain wallets", empty: "More wallets are coming." },
];

export const BROKERS: BrokerDefinition[] = [
  {
    id: "binance",
    name: "Binance",
    logo: "/logos/binance.svg",
    kind: "crypto",
    summary: "Spot account · read-only API key",
    instructions: [
      "In Binance, go to Account → API Management → Create API.",
      "Turn on only “Enable Reading”.",
      "Linvesther can never trade or withdraw — keys with those permissions are rejected.",
    ],
    fields: [
      { key: "api-key", label: "API key", secret: true },
      { key: "api-secret", label: "API secret", secret: true },
    ],
    connect: (accountId, values, label) => connectBinance(accountId, values["api-key"] ?? "", values["api-secret"] ?? "", label),
  },
  {
    id: "coinbase",
    name: "Coinbase",
    logo: "/logos/coinbase.svg",
    kind: "crypto",
    summary: "Advanced Trade · read-only API key",
    instructions: [
      "In Coinbase, open the Developer Platform (portal.cdp.coinbase.com) → API keys → Create API key.",
      "Give it only the “View” permission. Either signature type works (Ed25519 or ECDSA).",
      "Copy the key ID (or name) and the secret (or private key) it shows once. Keys that can trade or transfer are rejected.",
    ],
    fields: [
      { key: "key-name", label: "API key ID (or name)", secret: false },
      { key: "private-key", label: "Secret (or private key)", secret: true, multiline: true },
    ],
    connect: (accountId, values, label) => connectCoinbase(accountId, values["key-name"] ?? "", values["private-key"] ?? "", label),
  },
  {
    id: "kraken",
    name: "Kraken",
    logo: "/logos/kraken-mark.svg",
    kind: "crypto",
    summary: "Spot account · read-only API key",
    instructions: [
      "In Kraken, open Security → API (or Settings → API) and create a new API key.",
      "Tick only these four, which sit in different groups: under Funds “Query funds”; under Orders and trades “Query open orders & trades” and “Query closed orders & trades”; and under Data “Query ledger entries”.",
      "Copy the API key and the private key (Kraken shows the private key once). Keys that can place orders or withdraw are rejected.",
    ],
    fields: [
      { key: "api-key", label: "API key", secret: true },
      { key: "api-secret", label: "Private key", secret: true },
    ],
    connect: (accountId, values, label) => connectKraken(accountId, values["api-key"] ?? "", values["api-secret"] ?? "", label),
  },
  {
    id: "ibkr",
    name: "Interactive Brokers",
    logo: "/logos/ibkr.png",
    roundedLogo: true,
    kind: "stocks",
    summary: "Flex Web Service · read-only report token",
    instructions: [
      "In Account Management, go to Settings → Reporting → Flex Queries and create an Activity Flex Query with Cash Transactions, Trades and Equity Summary sections, daily period.",
      "In Settings → Reporting → Flex Web Service, generate a token — this is read-only and never lets Linvesther trade or move money.",
      "Copy the Flex Query ID (from the query you created) and the token.",
    ],
    fields: [
      { key: "query-id", label: "Flex Query ID", secret: false },
      { key: "token", label: "Flex Web Service token", secret: true },
    ],
    connect: (accountId, values, label) => connectIbkr(accountId, values["token"] ?? "", values["query-id"] ?? "", label),
  },
  {
    id: "wallet",
    name: "Crypto wallet",
    logo: "/logos/wallet.svg",
    kind: "onchain",
    method: "wallet",
    summary: "Ethereum, Base, Arbitrum, Optimism, Polygon · read-only",
    instructions: [
      "Choose the wallet installed in your browser and pick the address to follow.",
      "Sign one message to show the address is yours. It is not a transaction: it costs nothing and gives no access to your funds.",
      "The address stays private — your public profile shows percentages only, never the address.",
    ],
    fields: [],
    connect: (accountId, values, label) => connectWallet(accountId, values["message"] ?? "", values["signature"] ?? "", label),
  },
];

export const brokerById = (id: string) => BROKERS.find((broker) => broker.id === id);
