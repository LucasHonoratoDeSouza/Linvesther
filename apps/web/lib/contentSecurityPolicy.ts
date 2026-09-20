/** The page's Content-Security-Policy. Scripts run only if they carry this
 * request's nonce (and whatever those scripts load in turn), so markup
 * injected into a page cannot execute. */
export function contentSecurityPolicy(options: {
  nonce: string;
  apiUrl: string;
  development: boolean;
}): string {
  let apiOrigin = "";
  try {
    apiOrigin = new URL(options.apiUrl).origin;
  } catch {
    // An unparsable API address leaves connect-src at 'self' only.
  }
  const directives: Record<string, string[]> = {
    "default-src": ["'self'"],
    // Development tooling needs eval; a production page never does.
    "script-src": [
      "'self'",
      `'nonce-${options.nonce}'`,
      "'strict-dynamic'",
      ...(options.development ? ["'unsafe-eval'"] : []),
    ],
    // Inline styles are how the animation library moves things.
    "style-src": ["'self'", "'unsafe-inline'"],
    "img-src": ["'self'", "data:", "blob:"],
    "media-src": ["'self'", "blob:"],
    "font-src": ["'self'", "data:"],
    "connect-src": [
      "'self'",
      ...(apiOrigin ? [apiOrigin] : []),
      ...(options.development ? ["ws:", "wss:"] : []),
    ],
    "frame-ancestors": ["'none'"],
    "base-uri": ["'self'"],
    "object-src": ["'none'"],
    "form-action": ["'self'"],
    ...(options.development ? {} : { "upgrade-insecure-requests": [] }),
  };
  return Object.entries(directives)
    .map(([name, values]) => [name, ...values].join(" "))
    .join("; ");
}
