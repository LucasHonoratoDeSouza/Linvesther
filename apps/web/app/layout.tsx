import { headers } from "next/headers";
import { Instrument_Serif, Manrope } from "next/font/google";
import { Providers } from "../components/Providers";
import { SiteHeader } from "../components/SiteHeader";
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

export const metadata = {
  title: {
    default: "Linvesther — Performance. Proven privately.",
    template: "%s | Linvesther",
  },
  description:
    "Turn your financial track record into verifiable claims. Explore performance, choose what to share, and keep your trading history private.",
};

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // Reading the request headers makes every page render per request, which is
  // what lets the framework put this request's nonce on its own scripts.
  await headers();
  return (
    <html
      lang="en"
      className={`${instrumentSerif.variable} ${manrope.variable}`}
    >
      <body>
        <Providers>
          <a className="skip-link" href="#main-content">
            Skip to content
          </a>
          <SiteHeader />
          <div id="main-content" tabIndex={-1}>
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
