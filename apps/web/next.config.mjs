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
  async headers() {
    return [{ source: "/:path*", headers: securityHeaders }];
  },
};

export default nextConfig;
