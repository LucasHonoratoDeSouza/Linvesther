import { headers } from "next/headers";
import { originOf, zoneOfHost, type Zone } from "./siteMeta";

/** Who is asking: the host they used, the matching origin and which part of the site it serves. */
export async function requestSite(): Promise<{
  origin: string;
  zone: Zone;
  pathname: string;
}> {
  const incoming = await headers();
  const host = incoming.get("host");
  const rootDomain = process.env.ROOT_DOMAIN;
  return {
    origin: originOf(host, incoming.get("x-forwarded-proto"), rootDomain),
    zone: zoneOfHost(host, rootDomain),
    pathname: incoming.get("x-pathname") ?? "/",
  };
}
