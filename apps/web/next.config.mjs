import { hostRewrites } from "./lib/hostRewrites.mjs";

// Dev-only: this app now answers on three subdomains (landing/app/docs)
// plus access by LAN IP, all proxied to this one local dev server — Next
// blocks dev asset/HMR requests whose Origin it doesn't recognize
// (security default), which otherwise shows up as a page stuck loading.
const devOrigins = process.env.ROOT_DOMAIN
  ? [
      process.env.ROOT_DOMAIN,
      `app.${process.env.ROOT_DOMAIN}`,
      `docs.${process.env.ROOT_DOMAIN}`,
      `api.${process.env.ROOT_DOMAIN}`,
    ]
  : undefined;

/** @type {import('next').NextConfig} */
const securityHeaders = [
  { key: "X-Content-Type-Options", value: "nosniff" },
  // Nothing in the app is meant to be shown inside another site's page.
  { key: "X-Frame-Options", value: "DENY" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
  // Passkeys stay allowed; everything the app never uses is switched off.
  {
    key: "Permissions-Policy",
    value:
      "camera=(), microphone=(), geolocation=(), payment=(), usb=(), bluetooth=()",
  },
  ...(process.env.NODE_ENV === "production"
    ? [
        {
          key: "Strict-Transport-Security",
          value: "max-age=31536000; includeSubDomains",
        },
      ]
    : []),
];

const nextConfig = {
  // A production build can go to its own directory, so it does not disturb a
  // development server that is running from `.next`.
  distDir: process.env.NEXT_DIST_DIR ?? ".next",
  allowedDevOrigins: devOrigins,
  // The list of trusted collectors lives at the repository root (`trust/`),
  // shared with the command-line verifier.
  experimental: { externalDir: true },
  images: {
    // The default (60s) re-encodes the same image from scratch on almost
    // every request once traffic is light, which is what made images look
    // slow to load. Public images don't change without a new deploy, so a
    // long cache is safe; the optimizer still adapts size/format per request.
    minimumCacheTTL: 31536000,
  },
  async rewrites() {
    return hostRewrites(process.env.ROOT_DOMAIN);
  },
  async headers() {
    return [
      { source: "/:path*", headers: securityHeaders },
      // The video and its poster are static files (unlike public/images/*,
      // which the /_next/image optimizer already caches long — see above),
      // so without this they were re-sent in full on every visit.
      {
        source: "/video/:path*",
        headers: [{ key: "Cache-Control", value: "public, max-age=31536000, immutable" }],
      },
    ];
  },
};

export default nextConfig;
