"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import type { ReactNode } from "react";
import { Icon } from "./Icon";

const navigation: { href: string; label: string }[] = [
  { href: "/portfolio", label: "Portfolio" },
  { href: "/disclose", label: "Public profile" },
  { href: "/claims", label: "Claims" },
  { href: "/explorer", label: "Explorer" },
];

// Setup, not day-to-day workspace tools — once an account
// exists, these matter far less often than Portfolio/Explorer/Create a
// claim, so they sit apart, smaller and out of the way, instead of
// competing for attention at the same level.
const accountNavigation: { href: string; label: string }[] = [
  { href: "/onboarding", label: "Your identity" },
  { href: "/settings/api-tokens", label: "API tokens" },
];

const allNavigation = [...navigation, ...accountNavigation];

export function AppShell({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  return (
    <div className="workspace">
      <aside className="workspace-sidebar">
        <div className="eyebrow sidebar-label">
          Workspace <span>01</span>
        </div>
        <nav aria-label="Workspace navigation">
          {navigation.map((item) => (
            <Link
              href={item.href}
              key={item.href}
              className={
                pathname === item.href ||
                (item.href === "/explorer" && (pathname.startsWith("/profile/") || pathname.startsWith("/p/")))
                  ? "active"
                  : ""
              }
              aria-current={pathname === item.href ? "page" : undefined}
            >
              {item.label}
              <span className="sidebar-active-dot" />
            </Link>
          ))}
        </nav>
        <nav aria-label="Account navigation" className="sidebar-secondary-nav">
          {accountNavigation.map((item) => (
            <Link
              href={item.href}
              key={item.href}
              className={pathname === item.href ? "active" : ""}
              aria-current={pathname === item.href ? "page" : undefined}
            >
              {item.label}
            </Link>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <p>
            Your data stays yours.
            <br />
            <span>You choose what to disclose.</span>
          </p>
          <a href="/docs">
            Read the documentation <Icon name="diagonal" width={14} />
          </a>
        </div>
      </aside>
      <div className="workspace-main">
        <div className="workspace-breadcrumb">
          <span>Linvesther</span>
          <span>/</span>
          <span>
            {allNavigation.find((item) => item.href === pathname)?.label ||
              "Public profile"}
          </span>
        </div>
        {children}
      </div>
    </div>
  );
}
