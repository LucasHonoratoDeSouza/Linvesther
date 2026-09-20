import Link from "next/link";

export const metadata = {
  title: "Verifiable investment track records",
  description:
    "Why a trading track record is hard to trust, and what makes one verifiable instead of merely claimed.",
};

export default function Page() {
  return (
    <>
      <span className="eyebrow">Guide</span>
      <h1>Verifiable investment track records</h1>
      <p className="document-lead">
        A track record is easy to claim and hard to check. This guide explains what would make one verifiable, and what Linvesther does about it.
      </p>

      <section>
        <h2>The problem</h2>
        <p>
          A screenshot can be edited and a spreadsheet can be curated. A statement from a broker proves less than it seems, because it can be cropped, and a chosen account can be shown while a weaker one stays hidden. The usual way to prove more is to hand over the history itself, which means publishing balances, positions and trades that most people would rather keep private.
        </p>
        <p>
          So a reader of a track record faces two questions at once: is this real, and is it the whole picture? Neither is answered by a bigger screenshot.
        </p>
      </section>

      <section>
        <h2>What verifiable means here</h2>
        <p>
          Linvesther judges a result on five independent questions: origin (who supplied the data), coverage (does it span the period without unexplained gaps), calculation (is there proof the result follows from the data by a published method), registry (is it anchored in a public record) and availability (can the evidence still be reached). Being strong on one says nothing about another, so each is reported on its own.
        </p>
        <p>
          The record is a single combined curve of every connected account, counted from the day it was connected. That is what makes it hard to curate: an account that did badly cannot be left out. The only choice is whether the record is public at all.
        </p>
      </section>

      <section>
        <h2>What is published</h2>
        <p>
          Percentages only. The public record has no balance, no position, no trade and no exchange identifier, so a reader cannot tell whether an account holds a hundred dollars or a hundred thousand.
        </p>
        <p>
          Statements about the record are signed claims, and calculations can be backed by zero-knowledge proofs that anyone can verify without seeing the data behind them.
        </p>
      </section>

      <section>
        <h2>What it does not prove</h2>
        <p>
          A proof shows a calculation is right, not that the data was honest. Today the data comes from a collector that reads an exchange with a read-only key and signs what it saw. That is not the exchange&apos;s own signature, and it does not resist a malicious collector: anyone can run one, and its proofs show as self-attested unless a verifier chooses to trust it. Linvesther says this plainly wherever a result is shown.
        </p>
      </section>

      <section>
        <h2>Keep reading</h2>
        <ul>
          <li>
            <Link href="/docs/concepts">How it works</Link>
          </li>
          <li>
            <Link href="/docs/zero-knowledge-trading-performance">Zero-knowledge performance</Link>
          </li>
          <li>
            <Link href="/docs/track-record-verification-approaches">Approaches compared</Link>
          </li>
        </ul>
      </section>
    </>
  );
}
