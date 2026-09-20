import { DOC_PAGES } from "../app/docs/nav";

export const REPOSITORY = "https://github.com/LucasHonoratoDeSouza/Linvesther";

export type Zone = "app" | "docs" | "landing";

/** Which part of the site a host serves: the apex is the landing page, `app.`
 * the product and `docs.` the documentation. */
export function zoneOfHost(
  host: string | null,
  rootDomain: string | undefined,
): Zone {
  const hostname = (host ?? "").toLowerCase().split(":")[0];
  if (rootDomain && hostname === `app.${rootDomain}`) return "app";
  if (rootDomain && hostname === `docs.${rootDomain}`) return "docs";
  return "landing";
}

/** The address a visitor used, as an origin ("https://docs.example.test"). */
export function originOf(
  host: string | null,
  protocol: string | null,
  rootDomain: string | undefined,
): string {
  const scheme = protocol === "http" ? "http" : "https";
  if (host) return `${scheme}://${host}`;
  return rootDomain ? `https://${rootDomain}` : "http://localhost:4300";
}

/** The origin of the documentation, wherever the visitor is. */
export function docsOriginOf(
  origin: string,
  rootDomain: string | undefined,
): string {
  if (!rootDomain) return origin;
  const scheme = origin.startsWith("http://") ? "http" : "https";
  return `${scheme}://docs.${rootDomain}`;
}

/** Search engines and AI crawlers are welcome on the website and the docs.
 * The app is a set of private pages with nothing to index. */
export function robotsFor(
  zone: Zone,
  origin: string,
): {
  rules: { userAgent: string; allow?: string; disallow?: string };
  sitemap?: string;
} {
  if (zone === "app") return { rules: { userAgent: "*", disallow: "/" } };
  return {
    rules: { userAgent: "*", allow: "/" },
    sitemap: `${origin}/sitemap.xml`,
  };
}

/** The pages a sitemap on this host lists. A sitemap may only name URLs on its own host. */
export function sitemapPaths(zone: Zone): string[] {
  if (zone === "app") return [];
  if (zone === "docs")
    return [
      ...DOC_PAGES.map((page) => page.href),
      "/whitepaper",
      "/linvesther-whitepaper.pdf",
    ];
  return ["/"];
}

/** RFC 9116: how to report a vulnerability. `expires` is set well inside the
 * one year the standard recommends, so the file never looks stale. */
export function securityTxt(origin: string, now: Date): string {
  const expires = new Date(
    now.getTime() + 300 * 24 * 60 * 60 * 1000,
  ).toISOString();
  return [
    `Contact: ${REPOSITORY}/security/advisories/new`,
    `Expires: ${expires}`,
    "Preferred-Languages: en, pt",
    `Policy: ${REPOSITORY}/blob/main/SECURITY.md`,
    `Canonical: ${origin}/.well-known/security.txt`,
    "",
  ].join("\n");
}

const SUMMARY =
  "Turn a real track record into public claims anyone can check, without publishing a balance, a position or a trade. " +
  "Performance is measured from read-only exchange and broker connections and published as percentages only. " +
  "Signed claims and zero-knowledge proofs back the statements, and identities are recorded in a public registry on Base (currently the Sepolia test network).";

/** llms.txt: what the project is and where the documentation lives. */
export function llmsTxt(docsOrigin: string, siteOrigin: string): string {
  const lines = [
    "# Linvesther",
    "",
    `> ${SUMMARY}`,
    "",
    "## Documentation",
    ...DOC_PAGES.map((page) => `- [${page.label}](${docsOrigin}${page.href})`),
    "",
    "## Paper",
    `- [Whitepaper (web)](${docsOrigin}/whitepaper)`,
    `- [Whitepaper (PDF)](${docsOrigin}/linvesther-whitepaper.pdf)`,
    "",
    "## Source",
    `- [Repository](${REPOSITORY})`,
    `- [Architecture](${REPOSITORY}/blob/main/docs/architecture.md)`,
    `- [Security policy](${REPOSITORY}/blob/main/SECURITY.md)`,
    "",
    "## Optional",
    `- [Full text of the project documentation](${siteOrigin}/llms-full.txt)`,
    "",
  ];
  return lines.join("\n");
}

/** Structured data for the landing page. */
export function organizationJsonLd(origin: string): object[] {
  return [
    {
      "@context": "https://schema.org",
      "@type": "Organization",
      name: "Linvesther",
      url: origin,
      logo: `${origin}/icons/icon-512.png`,
      sameAs: [REPOSITORY],
    },
    {
      "@context": "https://schema.org",
      "@type": "WebSite",
      name: "Linvesther",
      url: origin,
      description: SUMMARY,
    },
    {
      "@context": "https://schema.org",
      "@type": "SoftwareSourceCode",
      name: "Linvesther",
      codeRepository: REPOSITORY,
      license: "https://www.apache.org/licenses/LICENSE-2.0",
      programmingLanguage: ["TypeScript", "Rust", "Solidity"],
    },
  ];
}

/** JSON for a `<script type="application/ld+json">`, safe to place in HTML. */
export function jsonLdScript(data: object | object[]): string {
  return JSON.stringify(data).replace(/</g, "\\u003c");
}
