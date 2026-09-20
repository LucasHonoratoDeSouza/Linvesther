"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { DOC_PAGES } from "./nav";

const GROUPS = ["Start", "Concepts", "Guides", "Reference"] as const;

export function DocsSidebar() {
  const pathname = usePathname();
  return (
    <aside className="docs-sidebar">
      <nav aria-label="Documentation">
        {GROUPS.map((group) => (
          <div key={group} className="docs-group">
            <span className="docs-group-label">{group}</span>
            {DOC_PAGES.filter((page) => page.group === group).map((page) => (
              <Link
                key={page.href}
                href={page.href}
                className={pathname === page.href ? "active" : ""}
                aria-current={pathname === page.href ? "page" : undefined}
              >
                {page.label}
              </Link>
            ))}
          </div>
        ))}
        <div className="docs-group">
          <span className="docs-group-label">Paper</span>
          <Link href="/whitepaper">Whitepaper</Link>
        </div>
      </nav>
    </aside>
  );
}

export function DocsPager() {
  const pathname = usePathname();
  const index = DOC_PAGES.findIndex((page) => page.href === pathname);
  if (index === -1) return null;
  const previous = DOC_PAGES[index - 1];
  const next = DOC_PAGES[index + 1];
  return (
    <nav className="docs-pager" aria-label="Previous and next page">
      {previous ? (
        <Link href={previous.href}>
          <span>Previous</span>
          {previous.label}
        </Link>
      ) : (
        <span />
      )}
      {next && (
        <Link href={next.href} className="docs-pager-next">
          <span>Next</span>
          {next.label}
        </Link>
      )}
    </nav>
  );
}
