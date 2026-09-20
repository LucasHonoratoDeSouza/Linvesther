"use client";

import {
  AnimatePresence,
  motion,
  useInView,
  useReducedMotion,
} from "motion/react";
import Link from "next/link";
import { useEffect, useId, useRef, useState } from "react";
import { Icon } from "./Icon";
import styles from "./ProofPlayground.module.css";

const examples = [
  {
    label: "Return",
    metric: "Time-weighted return",
    operator: "≥",
    unit: "%",
    thresholds: [10, 15, 18],
    actual: "+18.42%",
    description: "Your return, without your trades.",
    path: "M0 84L4 84L7 80L11 80L15 80L19 83L22 82L26 76L30 73L34 67L37 64L41 61L45 59L48 65L52 60L56 56L60 53L63 59L67 65L71 68L75 69L78 66L82 65L86 61L90 63L93 60L97 57L101 59L104 49L108 46L112 39L116 40L119 42L123 43L127 42L131 37L134 35L138 36L142 39L145 40L149 33L153 35L157 33L160 29L164 35L168 33L172 26L175 34L179 34L183 33L187 35L190 32L194 30L198 36L201 31L205 26L209 21L213 13L216 10L220 8L224 12L228 8L231 10L235 10",
  },
  {
    label: "Drawdown",
    metric: "Maximum drawdown",
    operator: "≤",
    unit: "%",
    thresholds: [8, 10, 15],
    actual: "7.82%",
    description: "Your risk limit, without your positions.",
    path: "M0 41L4 43L7 41L11 37L15 37L19 39L22 38L26 35L30 31L34 33L37 36L41 32L45 29L48 23L52 19L56 18L60 18L63 8L67 8L71 9L75 11L78 9L82 15L86 21L90 27L93 32L97 36L101 38L104 43L108 54L112 58L116 68L119 74L123 84L127 83L131 84L134 82L138 70L142 69L145 65L149 68L153 67L157 64L160 63L164 61L168 59L172 61L175 58L179 59L183 52L187 52L190 48L194 47L198 43L201 43L205 39L209 38L213 33L216 30L220 28L224 29L228 25L231 22L235 16",
  },
  {
    label: "Sharpe",
    metric: "Sharpe ratio",
    operator: "≥",
    unit: "",
    thresholds: [1, 1.5, 2],
    actual: "2.31",
    description: "Your return per unit of risk, without your trades.",
    path: "M0 84L4 82L7 75L11 76L15 71L19 69L22 68L26 59L30 57L34 55L37 50L41 44L45 42L48 38L52 39L56 38L60 37L63 40L67 43L71 46L75 45L78 43L82 42L86 40L90 42L93 41L97 38L101 33L104 34L108 33L112 38L116 37L119 43L123 45L127 39L131 45L134 40L138 37L142 36L145 32L149 28L153 22L157 21L160 21L164 21L168 22L172 20L175 20L179 14L183 19L187 20L190 22L194 27L198 18L201 24L205 23L209 22L213 15L216 19L220 13L224 14L228 12L231 12L235 8",
  },
] as const;
const steps = [
  "Select statement",
  "Keep details private",
  "Preview disclosure",
];

export function ProofPlayground() {
  const [active, setActive] = useState(0);
  const [choice, setChoice] = useState(0);
  const [view, setView] = useState<"public" | "private">("public");
  const [stage, setStage] = useState(2);
  const [manual, setManual] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { amount: 0.25 });
  const reduced = useReducedMotion();
  const id = useId();
  const example = examples[active]!;
  const threshold = example.thresholds[choice]!;
  const protecting = view === "public" && stage >= 1;
  const showReceipt = view === "public" && stage === 2;

  useEffect(() => {
    if (manual) return;
    if (!inView || reduced || view === "private") {
      setStage(2);
      return;
    }
    setStage(0);
    const protect = window.setTimeout(() => setStage(1), 850);
    const preview = window.setTimeout(() => setStage(2), 1750);
    return () => {
      window.clearTimeout(protect);
      window.clearTimeout(preview);
    };
  }, [active, choice, manual, inView, reduced, view]);

  function selectExample(index: number) {
    setActive(index);
    setChoice(index === 0 ? 0 : 2);
    setManual(false);
  }

  return (
    <div className={styles.playground} ref={ref}>
      <div className={styles.controls}>
        <h3>Choose what to share.</h3>
        <div
          className={styles.options}
          role="group"
          aria-label="Example statement"
        >
          {examples.map((item, index) => (
            <button
              key={item.label}
              aria-pressed={active === index}
              onClick={() => selectExample(index)}
            >
              {active === index && (
                <motion.span
                  className={styles.selectedOption}
                  layoutId={`${id}-option`}
                  transition={{ type: "spring", stiffness: 380, damping: 32 }}
                />
              )}
              <span>{item.label}</span>
            </button>
          ))}
        </div>
        <div className={styles.explanation}>
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={active}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -5 }}
              transition={{ duration: 0.2 }}
            >
              <p>{example.description}</p>
            </motion.div>
          </AnimatePresence>
        </div>
        <fieldset className={styles.thresholds}>
          <legend>Set a threshold</legend>
          <div>
            {example.thresholds.map((value, index) => (
              <button
                key={value}
                aria-pressed={choice === index}
                onClick={() => {
                  setChoice(index);
                  setManual(false);
                }}
                aria-label={`Threshold ${value}${example.unit === "%" ? " percent" : ""}`}
              >
                {example.operator} {value}
                {example.unit}
              </button>
            ))}
          </div>
        </fieldset>
        <Link className={styles.createLink} href="/disclose">
          Create your own claim <Icon name="diagonal" width={15} />
        </Link>
      </div>

      <div className={styles.demonstration}>
        <div className={styles.studio}>
          <div className={styles.toolbar}>
            <span className={styles.studioMark}>Disclosure studio</span>
          </div>
          <div
            className={styles.visibility}
            role="group"
            aria-label="Preview visibility"
          >
            <button
              aria-pressed={view === "public"}
              onClick={() => {
                setView("public");
                setManual(false);
              }}
            >
              What you share
              {view === "public" && (
                <motion.span layoutId={`${id}-visibility`} />
              )}
            </button>
            <button
              aria-pressed={view === "private"}
              onClick={() => {
                setView("private");
                setManual(false);
              }}
            >
              What stays private
              {view === "private" && (
                <motion.span layoutId={`${id}-visibility`} />
              )}
            </button>
          </div>
          <div
            className={styles.scene}
            data-stage={stage}
            data-visibility={view}
          >
            <motion.div
              className={styles.source}
              animate={{
                x: view === "private" ? 25 : 0,
                y: showReceipt ? -9 : 0,
                opacity: showReceipt ? 0.68 : 1,
              }}
              transition={{ type: "spring", stiffness: 150, damping: 24 }}
            >
              <div className={styles.sourceHeader}>
                <span>
                  Your track record<small>Jan 01 — Apr 30, 2026</small>
                </span>
                <Icon name="lock" width={13} />
              </div>
              <div className={styles.sourceValue}>
                <span>{example.metric}</span>
                <motion.strong
                  animate={{
                    filter: protecting ? "blur(7px)" : "blur(0px)",
                    opacity: protecting ? 0.45 : 1,
                  }}
                  aria-hidden={protecting}
                >
                  {example.actual}
                </motion.strong>
              </div>
              <svg
                viewBox="0 0 235 88"
                className={styles.chart}
                aria-hidden="true"
              >
                <path
                  d="M0 80H235M0 45H235M0 10H235"
                  stroke="#353635"
                  strokeDasharray="2 5"
                />
                <motion.path
                  key={active}
                  d={example.path}
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  initial={{ pathLength: reduced ? 1 : 0 }}
                  animate={{ pathLength: 1, opacity: protecting ? 0.2 : 0.9 }}
                  transition={{
                    pathLength: { duration: 0.9 },
                    opacity: { duration: 0.4 },
                  }}
                />
              </svg>
              <div className={styles.privateRows}>
                {[
                  { label: "Account balance", value: "128,430.82 USDT" },
                  { label: "Positions & trades", value: "42 positions" },
                  { label: "Exchange credentials", value: "•••• •••• ••••" },
                ].map((row, index) => (
                  <div key={row.label}>
                    <span>{row.label}</span>
                    <motion.span
                      animate={{
                        filter: protecting ? "blur(6px)" : "blur(0px)",
                        opacity: protecting ? 0.35 : 0.8,
                      }}
                      transition={{ delay: reduced ? 0 : index * 0.09 }}
                      aria-hidden={protecting}
                    >
                      {row.value}
                    </motion.span>
                    <Icon name="lock" width={10} />
                  </div>
                ))}
              </div>
            </motion.div>

            <AnimatePresence>
              {view === "public" && stage === 1 && (
                <motion.div
                  className={styles.privacyMessage}
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -8 }}
                >
                  <Icon name="lock" width={14} />
                  Details stay private
                </motion.div>
              )}
            </AnimatePresence>
            <AnimatePresence>
              {showReceipt && (
                <motion.div
                  className={styles.receipt}
                  data-testid="claim-preview-receipt"
                  initial={{
                    opacity: 0,
                    y: reduced ? 0 : 45,
                    rotate: reduced ? 0 : 3,
                    scale: reduced ? 1 : 0.96,
                  }}
                  animate={{ opacity: 1, y: 0, rotate: 0, scale: 1 }}
                  exit={{
                    opacity: 0,
                    y: reduced ? 0 : 18,
                    scale: reduced ? 1 : 0.98,
                  }}
                  transition={{ type: "spring", stiffness: 170, damping: 24 }}
                >
                  <div className={styles.receiptTop}>
                    <span>
                      <Icon name="globe" width={13} />
                      Public statement
                    </span>
                    <span>Preview</span>
                  </div>
                  <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ delay: reduced ? 0 : 0.15 }}
                  >
                    <p className={styles.receiptMetric}>{example.metric}</p>
                    <div
                      className={styles.statement}
                      data-testid="claim-preview-statement"
                    >
                      <span>{example.operator}</span> {threshold}
                      <small>{example.unit}</small>
                    </div>
                  </motion.div>
                  <div className={styles.receiptPeriod}>
                    <span>Observation period</span>
                    <strong>Jan 01 — Apr 30, 2026</strong>
                  </div>
                  <div className={styles.receiptBottom}>
                    <span>
                      0x3a7f…8c2D<small>Public claim address</small>
                    </span>
                    <Icon name="arrow" width={15} />
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
            <AnimatePresence>
              {view === "private" && (
                <motion.div
                  className={styles.privateNote}
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0 }}
                >
                  <Icon name="lock" width={15} />
                  <span>The details remain yours.</span>
                </motion.div>
              )}
            </AnimatePresence>
          </div>
          <div className={styles.playback}>
            <div
              className={styles.steps}
              role="group"
              aria-label="Disclosure walkthrough"
            >
              {steps.map((label, index) => (
                <button
                  key={label}
                  aria-label={label}
                  aria-pressed={stage === index}
                  onClick={() => {
                    setView("public");
                    setManual(true);
                    setStage(index);
                  }}
                >
                  <span className={stage >= index ? styles.stepComplete : ""}>
                    {stage > index ? (
                      <Icon name="check" width={10} />
                    ) : (
                      index + 1
                    )}
                  </span>
                  <span>{["Select", "Protect", "Preview"][index]}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
        <p className={styles.disclaimer}>
          Illustrative preview · No proof generated.
        </p>
      </div>
    </div>
  );
}
