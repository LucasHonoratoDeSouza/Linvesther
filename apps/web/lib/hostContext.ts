/** Where a request came from, so pages can say the right thing. On the app
 * host the visitor is already in the app, so the header offers a way back to
 * the website instead of a link to the app they are looking at. */
export function hostContext(
  host: string | null,
  rootDomain: string | undefined,
  protocol: string | null,
): { onAppHost: boolean; websiteUrl: string } {
  const hostname = (host ?? "").toLowerCase().split(":")[0];
  const onAppHost = Boolean(rootDomain) && hostname === `app.${rootDomain}`;
  const scheme = protocol === "http" ? "http" : "https";
  return {
    onAppHost,
    websiteUrl: rootDomain ? `${scheme}://${rootDomain}` : "/",
  };
}
