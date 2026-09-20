import { describe, expect, it } from "vitest";
import { isStaticAsset } from "../../../apps/web/lib/staticAssets";

describe("which paths are files rather than pages", () => {
  it("treats files from public/ as files, on any host", () => {
    for (const path of ["/logos/binance.svg", "/logos/ibkr.png", "/images/proof-hero.png", "/video/linvesther-walkthrough.mp4", "/icons/icon-192.png", "/favicon.ico", "/manifest.webmanifest", "/linvesther-whitepaper.pdf", "/robots.txt", "/sitemap.xml", "/llms.txt"]) {
      expect(isStaticAsset(path), path).toBe(true);
    }
  });

  it("treats pages as pages, including addresses and identifiers", () => {
    for (const path of ["/", "/docs", "/docs/api", "/portfolio", "/p/0x1111111111111111111111111111111111111111", "/c/7a34269267a81302", "/profile/track-1", "/onboarding"]) {
      expect(isStaticAsset(path), path).toBe(false);
    }
  });
});
