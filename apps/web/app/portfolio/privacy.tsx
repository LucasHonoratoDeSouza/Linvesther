"use client";

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import styles from "./portfolio.module.css";

const STORAGE_KEY = "lz-hide-values";
const HIDDEN_TEXT = "••••";

interface Privacy {
  hidden: boolean;
  toggle: () => void;
  /** Replaces an absolute amount with dots while values are hidden. */
  mask: (text: string) => string;
  /** The gain/loss colour class — dropped while hidden, since the colour alone would still say which way it went. */
  tone: (value: string | number | null) => string;
}

const PrivacyContext = createContext<Privacy>({ hidden: false, toggle: () => {}, mask: (t) => t, tone: () => "" });

/** "Hide values": absolute amounts (balances, profit in money, holdings) can be
 * masked for a screen share or a look over the shoulder; percentages stay
 * visible. The choice is remembered on this device only. */
export function PrivacyProvider({ children }: { children: ReactNode }) {
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    try {
      setHidden(window.localStorage.getItem(STORAGE_KEY) === "1");
    } catch {
      // storage unavailable — values simply stay shown
    }
  }, []);

  const toggle = () =>
    setHidden((current) => {
      try {
        window.localStorage.setItem(STORAGE_KEY, current ? "0" : "1");
      } catch {
        // not persisted, still applied for this visit
      }
      return !current;
    });

  const value: Privacy = {
    hidden,
    toggle,
    mask: (text) => (hidden ? HIDDEN_TEXT : text),
    tone: (v) => (hidden || v === null || Number(v) === 0 ? "" : (Number(v) > 0 ? styles.pnlUp : styles.pnlDown) ?? ""),
  };
  return <PrivacyContext.Provider value={value}>{children}</PrivacyContext.Provider>;
}

export const usePrivacy = () => useContext(PrivacyContext);

export function EyeToggle() {
  const { hidden, toggle } = usePrivacy();
  return (
    <button
      type="button"
      className={styles.eyeButton}
      onClick={toggle}
      aria-pressed={hidden}
      aria-label={hidden ? "Show amounts" : "Hide amounts"}
      title={hidden ? "Show amounts" : "Hide amounts"}
      data-testid="hide-values-toggle"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Z" />
        <circle cx="12" cy="12" r="3" />
        {hidden && <path d="M4 4l16 16" />}
      </svg>
    </button>
  );
}
