import type { MetadataRoute } from "next";
import { requestSite } from "../lib/requestOrigin";
import { robotsFor } from "../lib/siteMeta";

export default async function robots(): Promise<MetadataRoute.Robots> {
  const { origin, zone } = await requestSite();
  return robotsFor(zone, origin);
}
