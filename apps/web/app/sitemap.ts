import type { MetadataRoute } from "next";
import { requestSite } from "../lib/requestOrigin";
import { sitemapPaths } from "../lib/siteMeta";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const { origin, zone } = await requestSite();
  return sitemapPaths(zone).map((path) => ({
    url: `${origin}${path}`,
    changeFrequency: "monthly" as const,
    priority: path === "/" || path === "/docs" ? 1 : 0.7,
  }));
}
