import { describe, expect, it } from "vitest";
import { docsOriginOf, jsonLdScript, llmsTxt, originOf, organizationJsonLd, REPOSITORY, robotsFor, securityTxt, sitemapPaths, zoneOfHost } from "../../../apps/web/lib/siteMeta";

const ROOT = "example.test";

describe("which part of the site a host serves", () => {
  it("tells the apex, the app and the docs apart, and nothing else", () => {
    expect(zoneOfHost("example.test", ROOT)).toBe("landing");
    expect(zoneOfHost("app.example.test:443", ROOT)).toBe("app");
    expect(zoneOfHost("DOCS.example.test", ROOT)).toBe("docs");
    expect(zoneOfHost("app.example.test.evil.test", ROOT)).toBe("landing");
    expect(zoneOfHost("app.localhost", undefined)).toBe("landing");
  });

  it("builds the origin a visitor used", () => {
    expect(originOf("docs.example.test", "https", ROOT)).toBe("https://docs.example.test");
    expect(originOf("localhost:4300", "http", undefined)).toBe("http://localhost:4300");
    expect(docsOriginOf("https://example.test", ROOT)).toBe("https://docs.example.test");
  });
});

describe("robots.txt", () => {
  it("welcomes crawlers to the website and the docs and points at the sitemap", () => {
    for (const zone of ["landing", "docs"] as const) {
      expect(robotsFor(zone, "https://x.test")).toEqual({ rules: { userAgent: "*", allow: "/" }, sitemap: "https://x.test/sitemap.xml" });
    }
  });

  it("keeps crawlers out of the app", () => {
    expect(robotsFor("app", "https://app.x.test")).toEqual({ rules: { userAgent: "*", disallow: "/" } });
  });
});

describe("sitemap.xml", () => {
  it("lists every documentation page and the whitepaper on the docs host", () => {
    const paths = sitemapPaths("docs");
    expect(paths).toContain("/docs/verify");
    expect(paths).toContain("/docs/self-hosting");
    expect(paths).toContain("/whitepaper");
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("lists the front page on the apex and nothing on the app host", () => {
    expect(sitemapPaths("landing")).toEqual(["/"]);
    expect(sitemapPaths("app")).toEqual([]);
  });
});

describe("security.txt", () => {
  it("names a contact, an expiry inside a year, the policy and its own address", () => {
    const now = new Date("2026-09-20T00:00:00Z");
    const text = securityTxt("https://example.test", now);
    expect(text).toContain(`Contact: ${REPOSITORY}/security/advisories/new`);
    expect(text).toContain(`Policy: ${REPOSITORY}/blob/main/SECURITY.md`);
    expect(text).toContain("Canonical: https://example.test/.well-known/security.txt");
    const expires = new Date(/Expires: (.+)/.exec(text)![1]!);
    const days = (expires.getTime() - now.getTime()) / 86_400_000;
    expect(days).toBeGreaterThan(200);
    expect(days).toBeLessThan(365);
  });
});

describe("llms.txt", () => {
  it("links every documentation page, the paper and the repository", () => {
    const text = llmsTxt("https://docs.example.test", "https://example.test");
    expect(text.startsWith("# Linvesther\n")).toBe(true);
    expect(text).toContain("[Verify a proof](https://docs.example.test/docs/verify)");
    expect(text).toContain("https://docs.example.test/linvesther-whitepaper.pdf");
    expect(text).toContain(REPOSITORY);
    expect(text).toContain("https://example.test/llms-full.txt");
  });
});

describe("structured data", () => {
  it("describes the organization, the website and the source", () => {
    const types = (organizationJsonLd("https://example.test") as { "@type": string }[]).map((item) => item["@type"]);
    expect(types).toEqual(["Organization", "WebSite", "SoftwareSourceCode"]);
  });

  it("cannot close its own script tag", () => {
    expect(jsonLdScript({ name: "</script><script>alert(1)</script>" })).not.toContain("</script>");
  });
});
