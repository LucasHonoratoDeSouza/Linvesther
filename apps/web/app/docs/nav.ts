export const DOC_PAGES = [
  { href: "/docs", label: "Overview", group: "Start" },
  { href: "/docs/getting-started", label: "Getting started", group: "Start" },
  { href: "/docs/connections", label: "Connect an account", group: "Start" },
  { href: "/docs/concepts", label: "How it works", group: "Concepts" },
  { href: "/docs/claims", label: "Claims", group: "Concepts" },
  { href: "/docs/security", label: "Security and limits", group: "Concepts" },
  { href: "/docs/verify", label: "Verify a proof", group: "Reference" },
  { href: "/docs/api", label: "Public API", group: "Reference" },
  { href: "/docs/mcp", label: "MCP server", group: "Reference" },
  { href: "/docs/contracts", label: "Contracts", group: "Reference" },
  { href: "/docs/self-hosting", label: "Run it yourself", group: "Reference" },
  { href: "/docs/faq", label: "FAQ and glossary", group: "Reference" },
  {
    href: "/docs/verifiable-investment-track-record",
    label: "Verifiable track records",
    group: "Guides",
    description:
      "Why a trading track record is hard to trust, and what makes one verifiable instead of merely claimed.",
  },
  {
    href: "/docs/zero-knowledge-trading-performance",
    label: "Zero-knowledge performance",
    group: "Guides",
    description:
      "How a zero-knowledge virtual machine can prove a trading result follows from private data without revealing that data.",
  },
  {
    href: "/docs/prove-performance-without-revealing-trades",
    label: "Prove it without revealing trades",
    group: "Guides",
    description:
      "How to show return and maximum drawdown to anyone while keeping balances, positions and trades private.",
  },
  {
    href: "/docs/track-record-verification-approaches",
    label: "Approaches compared",
    group: "Guides",
    description:
      "Verifiable trading track records: approaches, limitations and existing systems, from screenshots to zero-knowledge proofs.",
  },
] as const;
