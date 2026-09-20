import type { Metadata } from "next";
import { headers } from "next/headers";
import { Instrument_Serif, Manrope } from "next/font/google";
import { Providers } from "../components/Providers";
import { SiteHeader } from "../components/SiteHeader";
import { hostContext } from "../lib/hostContext";
import { requestSite } from "../lib/requestOrigin";
import { jsonLdScript, organizationJsonLd } from "../lib/siteMeta";
import "./globals.css";

const instrumentSerif = Instrument_Serif({
  subsets: ["latin"],
  weight: "400",
  style: ["normal", "italic"],
  variable: "--font-serif",
  display: "swap",
});

const manrope = Manrope({
  subsets: ["latin"],
  variable: "--font-sans",
  display: "swap",
});

const TITLE = "Linvesther — Performance. Proven privately.";
const DESCRIPTION =
  "Turn your financial track record into verifiable claims. Explore performance, choose what to share, and keep your trading history private.";

export async function generateMetadata(): Promise<Metadata> {
  const { origin, zone, pathname } = await requestSite();
  return {
    metadataBase: new URL(origin),
    title: { default: TITLE, template: "%s | Linvesther" },
    description: DESCRIPTION,
    alternates: { canonical: pathname },
    openGraph: {
      type: "website",
      siteName: "Linvesther",
      title: TITLE,
      description: DESCRIPTION,
      url: pathname,
    },
    twitter: {
      card: "summary_large_image",
      title: TITLE,
      description: DESCRIPTION,
    },
    // The app is private pages with nothing to index.
    ...(zone === "app" ? { robots: { index: false, follow: false } } : {}),
  };
}

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // Reading the request headers makes every page render per request, which is
  // what lets the framework put this request's nonce on its own scripts.
  const requestHeaders = await headers();
  const { onAppHost, websiteUrl } = hostContext(
    requestHeaders.get("host"),
    process.env.ROOT_DOMAIN,
    requestHeaders.get("x-forwarded-proto"),
  );
  const site = await requestSite();
  return (
    <html
      lang="en"
      className={`${instrumentSerif.variable} ${manrope.variable}`}
    >
      <body>
        {site.zone === "landing" && (
          <script
            type="application/ld+json"
            dangerouslySetInnerHTML={{
              __html: jsonLdScript(organizationJsonLd(site.origin)),
            }}
          />
        )}
        <Providers>
          <a className="skip-link" href="#main-content">
            Skip to content
          </a>
          <SiteHeader onAppHost={onAppHost} websiteUrl={websiteUrl} />
          <div id="main-content" tabIndex={-1}>
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
