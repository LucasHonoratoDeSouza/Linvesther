import { NextResponse, type NextRequest } from "next/server";
import { contentSecurityPolicy } from "./lib/contentSecurityPolicy";

// Splits the landing page, the app (portfolio/explorer/disclose/…) and
// the docs (whitepaper/docs) across three subdomains of the same
// deployment: the apex domain is the landing page only, app.<domain>
// is the product, docs.<domain> is the documentation. Only active once
// ROOT_DOMAIN is set — local dev and access by IP address (no such env
// var) pass straight through unchanged.
const ROOT_DOMAIN = process.env.ROOT_DOMAIN;

const APP_PREFIXES = [
  "/portfolio",
  "/explorer",
  "/disclose",
  "/claims",
  "/c",
  "/onboarding",
  "/p",
  "/profile",
];
const DOCS_PREFIXES = ["/docs", "/whitepaper", "/linvesther-whitepaper.pdf"];

function zoneOf(pathname: string): "app" | "docs" | "landing" {
  if (APP_PREFIXES.some((p) => pathname === p || pathname.startsWith(`${p}/`)))
    return "app";
  if (DOCS_PREFIXES.some((p) => pathname === p || pathname.startsWith(`${p}/`)))
    return "docs";
  return "landing";
}

export function middleware(request: NextRequest) {
  // Every page gets its own nonce; the framework stamps it on the scripts it
  // serves because the policy is also on the request it renders.
  const nonce = btoa(crypto.randomUUID());
  const policy = contentSecurityPolicy({
    nonce,
    apiUrl: process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:4301",
    development: process.env.NODE_ENV !== "production",
  });
  const requestHeaders = new Headers(request.headers);
  requestHeaders.set("x-nonce", nonce);
  requestHeaders.set("content-security-policy", policy);
  const withPolicy = (response: NextResponse) => {
    response.headers.set("content-security-policy", policy);
    return response;
  };
  const next = () =>
    withPolicy(NextResponse.next({ request: { headers: requestHeaders } }));

  if (!ROOT_DOMAIN) return next();

  const host = request.headers.get("host") ?? "";
  if (!host.endsWith(ROOT_DOMAIN)) return next();

  const subdomain =
    host === ROOT_DOMAIN
      ? ""
      : host.slice(0, host.length - ROOT_DOMAIN.length - 1);
  const { pathname } = request.nextUrl;
  const zone = zoneOf(pathname);

  // Cross-host: a real redirect (a 3xx response the browser re-requests) —
  // never a rewrite, which Next would otherwise try to serve by having
  // the server fetch that other host itself. Built as a plain string,
  // not via request.nextUrl.clone(): NextURL's `.host` setter doesn't
  // clear an inherited `.port` (the dev server's own local port, e.g.
  // :4300, picked up from how it sees its own address behind the
  // tunnel), which otherwise leaks into the redirect target.
  const to = (targetHost: string, path: string) =>
    new URL(
      `${request.nextUrl.protocol}//${targetHost}${path}${request.nextUrl.search}`,
    );
  // Same-host: only the pathname changes, so a plain relative rewrite
  // (no host reassignment) is enough, and avoids Next treating it as an
  // external URL it would need to fetch itself.
  const rewriteTo = (path: string) =>
    withPolicy(
      NextResponse.rewrite(new URL(path, request.url), {
        request: { headers: requestHeaders },
      }),
    );

  if (subdomain === "app") {
    if (zone === "docs")
      return NextResponse.redirect(to(`docs.${ROOT_DOMAIN}`, pathname));
    if (zone === "landing" && pathname === "/") return rewriteTo("/portfolio");
    if (zone === "landing")
      return NextResponse.redirect(to(ROOT_DOMAIN, pathname));
    return next();
  }
  if (subdomain === "docs") {
    if (zone === "app")
      return NextResponse.redirect(to(`app.${ROOT_DOMAIN}`, pathname));
    if (zone === "landing" && pathname === "/") return rewriteTo("/docs");
    if (zone === "landing")
      return NextResponse.redirect(to(ROOT_DOMAIN, pathname));
    return next();
  }
  // The apex domain: landing content only, everything else points at its own subdomain.
  if (zone === "app")
    return NextResponse.redirect(to(`app.${ROOT_DOMAIN}`, pathname));
  if (zone === "docs")
    return NextResponse.redirect(to(`docs.${ROOT_DOMAIN}`, pathname));
  return next();
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
