"use client";

import { motion, useReducedMotion } from "motion/react";

/** Deterministic pseudo-random generator (mulberry32) so the dot field
 * is identical on server and client render — Math.random() here would
 * cause a hydration mismatch. */
function mulberry32(seed: number) {
  return function () {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

interface Dot {
  startX: number;
  endX: number;
  y: number;
  size: number;
  duration: number;
  pulseDelay: number;
  pulseDuration: number;
}

const DOT_COUNT = 140;
// Each dot travels one full container width, right to its own start +
// 100 (off-screen right), then loops back to its start position —
// staggered durations across 140 dots keep any single reset from
// reading as a pop in the overall field.
const DRIFT = 100;

const dots: Dot[] = (() => {
  const rand = mulberry32(42);
  const list: Dot[] = [];
  for (let i = 0; i < DOT_COUNT; i++) {
    // Loosely cluster along a shallow arc, like Hyperliquid's swarm,
    // rather than a uniform grid.
    const t = rand();
    const arcY = 50 - Math.sin(t * Math.PI) * 22;
    list.push({
      startX: t * 100,
      endX: t * 100 + DRIFT,
      y: arcY + (rand() - 0.5) * 46,
      size: 2 + rand() * 7,
      duration: 14 + rand() * 10,
      pulseDelay: rand() * 4,
      pulseDuration: 3 + rand() * 3,
    });
  }
  return list;
})();

/** Real animated particle field — no canvas/WebGL dependency, just
 * absolutely-positioned spans drifting left-to-right via `motion`,
 * combined with an independent opacity pulse per dot. Reduced motion
 * disables both, leaving a static field. */
export function ParticleBreak({
  label,
  title,
  body,
  cta,
}: {
  label: string;
  title: React.ReactNode;
  body?: React.ReactNode;
  cta?: React.ReactNode;
}) {
  const reduced = useReducedMotion();
  return (
    <div className="particle-break">
      <div className="particle-field" aria-hidden="true">
        {dots.map((dot, i) => (
          <motion.span
            key={i}
            className="particle-dot"
            style={{ top: `${dot.y}%`, width: dot.size, height: dot.size }}
            initial={{ left: `${dot.startX}%`, opacity: 0.15 }}
            animate={
              reduced
                ? { opacity: 0.35 }
                : { left: [`${dot.startX}%`, `${dot.endX}%`], opacity: [0.15, 0.85, 0.15] }
            }
            transition={
              reduced
                ? {}
                : {
                    left: { duration: dot.duration, repeat: Infinity, ease: "linear" },
                    opacity: { duration: dot.pulseDuration, delay: dot.pulseDelay, repeat: Infinity, ease: "easeInOut" },
                  }
            }
          />
        ))}
      </div>
      <div className="particle-break-copy">
        <span className="eyebrow">{label}</span>
        <h2>{title}</h2>
        {body && <p>{body}</p>}
        {cta && <div className="particle-break-cta">{cta}</div>}
      </div>
    </div>
  );
}
