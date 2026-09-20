/** The one address a request should have, or null when it already has it.
 * `www.<domain>` is the same site as `<domain>`, and a plain-http request is the
 * same page as its https one; both are sent to the single canonical address so
 * search engines do not see duplicates. */
export function canonicalRedirect(
  host: string | null,
  forwardedProto: string | null,
  rootDomain: string | undefined,
  pathAndQuery: string,
): string | null {
  if (!rootDomain) return null;
  const hostname = (host ?? "").toLowerCase().split(":")[0] ?? "";
  const onWww = hostname === `www.${rootDomain}`;
  const insecure = forwardedProto === "http";
  const ours = onWww || hostname === rootDomain || hostname.endsWith(`.${rootDomain}`);
  if (!ours || (!onWww && !insecure)) return null;
  return `https://${onWww ? rootDomain : hostname}${pathAndQuery}`;
}
