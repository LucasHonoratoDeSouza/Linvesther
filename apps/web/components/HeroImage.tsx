"use client";

import { motion, useReducedMotion } from "motion/react";
import Image from "next/image";

/** The landing page hero visual (apps/web/public/images/proof-hero.png). */
export function HeroImage() {
  const reduced = useReducedMotion();
  return (
    <motion.div
      className="hero-image-frame"
      initial={{ opacity: 0, y: 30 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.9, delay: 0.3, ease: [0.16, 1, 0.3, 1] }}
    >
      <motion.div
        animate={reduced ? {} : { y: [0, -10, 0] }}
        transition={{ duration: 7, repeat: Infinity, ease: "easeInOut" }}
      >
        <Image
          src="/images/proof-hero.png"
          alt="Linvesther dashboard showing wallet balance, Sharpe ratio, drawdown, win rate and a selective-disclosure proof panel"
          width={1714}
          height={918}
          priority
          style={{ width: "100%", height: "auto", display: "block" }}
        />
      </motion.div>
    </motion.div>
  );
}
