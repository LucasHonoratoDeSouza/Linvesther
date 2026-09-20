"use client";

import { usePathname } from "next/navigation";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { Brand } from "./Brand";
import { Icon } from "./Icon";

export const docsUrl = process.env.NEXT_PUBLIC_DOCS_URL || "/docs";

export function SiteHeader() {
  const pathname = usePathname();
  const [open, setOpen] = useState(false);
  const [protocolOpen, setProtocolOpen] = useState(false);
  const header = useRef<HTMLElement>(null);
  const protocolButton = useRef<HTMLButtonElement>(null);
  const mobileButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    setOpen(false);
    setProtocolOpen(false);
  }, [pathname]);
  useEffect(() => {
    function dismiss(event: PointerEvent) {
      if (!header.current?.contains(event.target as Node)) {
        setOpen(false);
        setProtocolOpen(false);
      }
    }
    function escape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        if (protocolOpen) protocolButton.current?.focus();
        else if (open) mobileButton.current?.focus();
        setOpen(false);
        setProtocolOpen(false);
      }
    }
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", escape);
    };
  }, [open, protocolOpen]);

  return (
    <header className="site-header" ref={header}>
      <div className="header-inner">
        <Brand />
        <nav className="desktop-nav" aria-label="Main navigation">
          <a href="/#how-it-works">How it works</a>
          <a
            href="/explorer"
            aria-current={pathname === "/explorer" ? "page" : undefined}
          >
            Explorer
          </a>
          <div
            className="nav-dropdown"
            onBlur={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget))
                setProtocolOpen(false);
            }}
          >
            <button
              ref={protocolButton}
              aria-expanded={protocolOpen}
              aria-controls="protocol-navigation"
              onClick={() => setProtocolOpen(!protocolOpen)}
            >
              Protocol <Icon name="chevron" width={13} />
            </button>
            <AnimatePresence>
              {protocolOpen && (
                <motion.div
                  id="protocol-navigation"
                  className="dropdown-panel"
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: 6 }}
                >
                  <a
                    href="/whitepaper"
                    onClick={() => setProtocolOpen(false)}
                  >
                    <Icon name="book" />
                    <span>
                      Whitepaper<small>The protocol, explained.</small>
                    </span>
                  </a>
                  <a
                    href="/docs/concepts#verification"
                    onClick={() => setProtocolOpen(false)}
                  >
                    <Icon name="shield" />
                    <span>
                      Verification<small>Understand every guarantee.</small>
                    </span>
                  </a>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
          <a href={docsUrl}>
            Docs <Icon name="diagonal" width={12} />
          </a>
        </nav>
        <div className="header-actions">
          <a href="/portfolio" className="button button-small button-light">
            Open app <Icon name="diagonal" width={14} />
          </a>
          <button
            className="icon-button mobile-menu-toggle"
            ref={mobileButton}
            aria-label={open ? "Close navigation" : "Open navigation"}
            aria-expanded={open}
            aria-controls="mobile-navigation"
            onClick={() => setOpen(!open)}
          >
            <Icon name={open ? "close" : "menu"} />
          </button>
        </div>
      </div>
      <AnimatePresence>
        {open && (
          <motion.nav
            id="mobile-navigation"
            className="mobile-navigation"
            aria-label="Mobile navigation"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
          >
            {[
              { href: "/#how-it-works", label: "How it works" },
              { href: "/explorer", label: "Explorer" },
              { href: "/whitepaper", label: "Whitepaper" },
              { href: docsUrl, label: "Documentation" },
            ].map((item) => (
              <a
                key={item.label}
                href={item.href}
                onClick={() => setOpen(false)}
              >
                {item.label}
                <Icon name="arrow" />
              </a>
            ))}
          </motion.nav>
        )}
      </AnimatePresence>
    </header>
  );
}
