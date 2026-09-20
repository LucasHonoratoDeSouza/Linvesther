"use client";

import { motion } from "motion/react";
import { Icon } from "../components/Icon";
import { HeroImage } from "../components/HeroImage";
import { ProofPlayground } from "../components/ProofPlayground";
import { TechSection } from "../components/TechSection";
import { ProcessSteps } from "../components/ProcessSteps";
import { MediaSlot } from "../components/MediaSlot";
import { ParticleBreak } from "../components/ParticleBreak";
import { GitHubButton } from "../components/GitHubLink";
import { AnimatedMark } from "../components/AnimatedMark";
import { SiteFooter } from "../components/SiteFooter";
import type { ReactNode } from "react";

function Reveal({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <motion.div
      className={className}
      initial={{ opacity: 0, y: 24 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: "-40px" }}
      transition={{ duration: 0.7 }}
    >
      {children}
    </motion.div>
  );
}

const questions = [
  {
    question: "What does Linvesther actually prove?",
    answer:
      "How your connected accounts performed since you connected them: return, worst drop, risk-adjusted return and win rate. A proof of calculation shows a result follows from its inputs; it does not by itself prove your exchange was honest or that these are all your accounts. Each of those limits stays visible.",
  },
  {
    question: "Will anyone see my balance or trades?",
    answer:
      "No. Your public profile adds all your connected accounts together and shows percentages only. Balances, positions, trades and credentials are never published. You can also switch on privacy mode, and anyone opening your address will see only that the profile is private.",
  },
  {
    question: "Does Linvesther have access to my funds?",
    answer:
      "No. Every connection is read-only: Binance and Coinbase keys with trading or withdrawal permissions are rejected, and Interactive Brokers works from a read-only report token. Your assets stay at the exchange. Your passkey or password identity is what signs on your behalf.",
  },
  {
    question: "Which accounts can I connect?",
    answer:
      "Binance, Coinbase and Interactive Brokers, in any combination. Crypto exchanges update within moments; Interactive Brokers reports once a day. We're always improving, and many more connections are on the way, so this list won't stay short.",
  },
  {
    question: "Is anything stored on a blockchain?",
    answer:
      "Your identity and the optional name and bio on your profile are recorded on Base, currently the Sepolia test network. Your trading data is not: it stays off-chain, and only proofs and percentages are ever shared.",
  },
  {
    question: "Is a signed claim the same as a ZK proof?",
    answer:
      "No. Signing authorizes what you disclose; a zero-knowledge proof separately shows the number is correct without revealing the trades behind it. Linvesther never labels a signature as a verified proof.",
  },
];

export default function HomePage() {
  return (
    <>
      <main className="landing">
        <section className="landing-hero container">
          <motion.a
            href="/whitepaper"
            className="hero-announcement"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
          >
            <span className="announcement-mark">L</span>A new standard for
            financial trust
            <Icon name="arrow" width={14} />
          </motion.a>
          <motion.h1
            initial={{ opacity: 0, y: 18 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.8, delay: 0.08 }}
          >
            Your performance.
            <br />
            <span>Proven. Privately.</span>
          </motion.h1>
          <motion.div
            className="hero-actions"
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.24 }}
          >
            <a href="/onboarding" className="button button-accent">
              Create or connect account <Icon name="diagonal" width={17} />
            </a>
            <a href="/explorer" className="button button-outline">
              Explorer <Icon name="arrow" width={17} />
            </a>
          </motion.div>
          <HeroImage />
        </section>
        <div className="principle-strip">
          <div className="container">
            <span>
              <Icon name="lock" width={16} />
              Private by design
            </span>
            <span>
              <Icon name="wallet" width={16} />
              Read-only connections
            </span>
            <span>
              <Icon name="shield" width={16} />
              Explicit guarantees
            </span>
            <span>
              <Icon name="globe" width={16} />
              Portable evidence
            </span>
          </div>
        </div>
        <ParticleBreak
          label="BUILT IN THE OPEN"
          title={
            <>
              Don't take our word for it.
              <br />
              <em>Read the source.</em>
            </>
          }
          cta={<GitHubButton />}
        />

        <section className="section container" id="selective-disclosure">
          <Reveal className="section-heading">
            <span className="eyebrow">Less exposure. More conviction.</span>
            <div className="heading-with-mark">
              <AnimatedMark size={60} />
              <div className="split-heading">
                <h2>
                  Your history is private.
                  <br />
                  <span>Your credibility doesn’t have to be.</span>
                </h2>
              </div>
            </div>
          </Reveal>
          <Reveal>
            <ProofPlayground />
          </Reveal>
        </section>

        <section className="process-section" id="how-it-works">
          <div className="container section">
            <Reveal className="section-heading">
              <h2>
                Let your work
                <br />
                <span>speak for itself.</span>
              </h2>
            </Reveal>
            <Reveal>
              <ProcessSteps />
            </Reveal>
          </div>
        </section>

        <TechSection />

        <section className="section container">
          <Reveal className="section-heading">
            <span className="eyebrow">
              One protocol. Different perspectives.
            </span>
            <h2>
              A place for every
              <br />
              <span>side of the proof.</span>
            </h2>
          </Reveal>
          <div className="product-paths">
            {[
              {
                number: "01",
                title: "Build your reputation.",
                text: "Connect your account, understand your performance and decide what to share.",
                href: "/portfolio",
                cta: "Your portfolio",
                icon: "chart" as const,
              },
              {
                number: "02",
                title: "Look beyond a number.",
                text: "Inspect a public track, its coverage, the source of its data and the limits of each guarantee.",
                href: "/explorer",
                cta: "Open explorer",
                icon: "search" as const,
              },
              {
                number: "03",
                title: "Build on the evidence.",
                text: "Understand the public projection, disclosure flow and verification model.",
                href: "/docs",
                cta: "Read the docs",
                icon: "code" as const,
              },
            ].map((item) => (
              <Reveal key={item.number}>
                <a href={item.href} className="product-path">
                  <div className="product-path-top">
                    <Icon name={item.icon} width={25} height={25} />
                    <span>{item.number}</span>
                  </div>
                  <h3>{item.title}</h3>
                  <p>{item.text}</p>
                  <span className="text-link">
                    {item.cta}
                    <Icon name="diagonal" width={17} />
                  </span>
                </a>
              </Reveal>
            ))}
          </div>
        </section>

        <section className="container film-section">
          <Reveal>
            <MediaSlot />
          </Reveal>
        </section>

        <section className="section container faq-section">
          <div>
            <span className="eyebrow">A little more clarity</span>
            <h2>
              Good questions.
              <br />
              <span>Clear answers.</span>
            </h2>
            <a href="/docs" className="text-link">
              Go deeper in the docs <Icon name="diagonal" width={17} />
            </a>
          </div>
          <div className="faq-list">
            {questions.map((item) => (
              <details key={item.question}>
                <summary>
                  {item.question}
                  <Icon name="plus" width={17} />
                </summary>
                <p>{item.answer}</p>
              </details>
            ))}
          </div>
        </section>

        <section className="closing-section container">
          <Reveal>
            <span className="eyebrow">Your next chapter is verifiable.</span>
            <h2>
              Make your
              <br />
              <em>track record matter.</em>
            </h2>
            <a href="/onboarding" className="button button-accent">
              Get started <Icon name="diagonal" width={17} />
            </a>
            <a className="closing-secondary" href="/whitepaper">
              Read the protocol first <Icon name="arrow" width={14} />
            </a>
          </Reveal>
          <span className="closing-watermark" aria-hidden="true">
            L
          </span>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
