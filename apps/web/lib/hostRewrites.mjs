/** What each subdomain shows at "/". The apex is the landing page; `app.` opens
 * the portfolio and `docs.` the documentation. Declared as config rewrites, not
 * in the middleware: a middleware rewrite is built from an absolute URL, and
 * behind a TLS-terminating proxy that URL came out as https://localhost:<port>,
 * which Next then tried to fetch over TLS and answered with a 500. */
export function hostRewrites(rootDomain) {
  if (!rootDomain) return [];
  return {
    beforeFiles: [
      {
        source: "/",
        has: [{ type: "host", value: `app.${rootDomain}` }],
        destination: "/portfolio",
      },
      {
        source: "/",
        has: [{ type: "host", value: `docs.${rootDomain}` }],
        destination: "/docs",
      },
    ],
  };
}
