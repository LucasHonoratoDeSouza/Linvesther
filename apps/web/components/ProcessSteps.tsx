"use client";

import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { EXCHANGES } from "./ExchangeMark";
import { Icon, type IconName } from "./Icon";

const steps: {
  title: string;
  body: string;
  icon: IconName;
  label: string;
  detail: string;
  rows: string[];
}[] = [
  {
    title: "Connect your history.",
    body: "Connect a Binance account with read-only credentials. Your funds stay where they are; the connection cannot place trades or withdraw.",
    icon: "wallet",
    label: "Read-only connection",
    detail: "Your exchange. Your custody.",
    rows: [
      "Read account history",
      "Reconstruct cash flows",
      "Keep custody at the source",
    ],
  },
  {
    title: "Make performance meaningful.",
    body: "Explore returns, drawdown and risk-adjusted metrics. Gaps and insufficient history remain visible, so the context stays with the numbers.",
    icon: "chart",
    label: "Performance with context",
    detail: "Every metric has a basis.",
    rows: [
      "Defined observation period",
      "Explicit coverage and gaps",
      "Unavailable when evidence is missing",
    ],
  },
  {
    title: "Choose what the world sees.",
    body: "Select a statement and sign the disclosure. A published claim and a verified ZK receipt are distinct steps, with their own evidence.",
    icon: "shield",
    label: "Selective disclosure",
    detail: "A statement. On your terms.",
    rows: [
      "Review the exact claim",
      "Sign with your passkey or password",
      "Share the disclosed result",
    ],
  },
];

export function ProcessSteps() {
  const [active, setActive] = useState(0);
  const selected = steps[active]!;
  return (
    <div className="process-layout">
      <div className="process-steps">
        {steps.map((step, index) => (
          <div
            key={step.title}
            className={`process-step${active === index ? " active" : ""}`}
          >
            <button
              aria-expanded={active === index}
              aria-controls={`process-detail-${index}`}
              onClick={() => setActive(index)}
            >
              <span className="process-number">0{index + 1}</span>
              <h3>{step.title}</h3>
              <Icon name={active === index ? "arrow" : "plus"} width={17} />
            </button>
            <AnimatePresence initial={false}>
              {active === index && (
                <motion.div
                  id={`process-detail-${index}`}
                  initial={{ height: 0, opacity: 0 }}
                  animate={{ height: "auto", opacity: 1 }}
                  exit={{ height: 0, opacity: 0 }}
                >
                  <p>{step.body}</p>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
        ))}
      </div>
      <div className="process-visual">
        <div className="process-visual-header">
          <span>THE PROTOCOL IN PRACTICE</span>
          <span>0{active + 1} / 03</span>
        </div>
        <AnimatePresence mode="wait">
          <motion.div
            className="process-visual-body"
            key={active}
            initial={{ opacity: 0, y: 15 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
          >
            {active === 0 ? (
              <div className="exchange-row" aria-label="Supported exchanges">
                {EXCHANGES.map((exchange) => (
                  <span
                    key={exchange.name}
                    className={`exchange-badge${exchange.status === "planned" ? " is-planned" : ""}`}
                    title={exchange.status === "connected" ? exchange.name : `${exchange.name} — planned, not yet available`}
                  >
                    {exchange.mark}
                    {exchange.name}
                    {exchange.status === "planned" && <em>Soon</em>}
                  </span>
                ))}
              </div>
            ) : (
              <div className="process-orbit" aria-hidden="true">
                <div />
                <div />
                <div />
                <span>
                  <Icon name={selected.icon} width={38} height={38} />
                </span>
              </div>
            )}
            <span className="eyebrow">{selected.label}</span>
            <h3>{selected.detail}</h3>
            <ul>
              {selected.rows.map((row) => (
                <li key={row}>
                  <Icon name="check" width={14} />
                  {row}
                </li>
              ))}
            </ul>
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}
