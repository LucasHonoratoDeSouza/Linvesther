// A synthetic, independent reference connector — its fixture data and
// bugs are written without looking at `suite.ts`'s implementation, per
// this project's rule against generating expected values from the same
// function under test.

import type { ConnectorContract, LedgerEventFixture, Page } from "../src/contract.js";

const FIXTURE_EVENTS: LedgerEventFixture[] = [
  { id: "evt-1", economicTimeMs: 1_000, symbol: "BTCUSDT", side: "buy" },
  { id: "evt-2", economicTimeMs: 2_000, symbol: "BTCUSDT", side: "sell" },
  { id: "evt-3", economicTimeMs: 3_000, symbol: "ETHUSDT", side: "buy" },
  { id: "evt-4", economicTimeMs: 4_000, symbol: "ETHUSDT", side: "sell" },
];

const REGISTERED_AT_MS = 500;

function inWindow(event: LedgerEventFixture, sinceMs: number, untilMs: number): boolean {
  return event.economicTimeMs >= sinceMs && event.economicTimeMs < untilMs;
}

/** A well-formed connector: honors windows and filters, never returns
 * anything before its own registration time. */
export function createConformantConnector(): ConnectorContract {
  return {
    registeredAt: REGISTERED_AT_MS,
    async fetchEvents(sinceMs, untilMs, filter) {
      const items = FIXTURE_EVENTS.filter((event) => inWindow(event, sinceMs, untilMs) && (!filter?.symbol || event.symbol === filter.symbol));
      return { items, complete: true };
    },
  };
}

/** Returns events from before its own registration as if they were
 * eligible history — the suite must reject this. */
export function createBackdatingConnector(): ConnectorContract {
  return {
    registeredAt: 2_500, // later than evt-1/evt-2's economicTimeMs
    async fetchEvents(sinceMs, untilMs, filter) {
      const items = FIXTURE_EVENTS.filter((event) => inWindow(event, sinceMs, untilMs) && (!filter?.symbol || event.symbol === filter.symbol));
      return { items, complete: true };
    },
  };
}

/** Silently ignores the `symbol` filter — the suite must reject this. */
export function createFilterIgnoringConnector(): ConnectorContract {
  return {
    registeredAt: REGISTERED_AT_MS,
    async fetchEvents(sinceMs, untilMs) {
      const items = FIXTURE_EVENTS.filter((event) => inWindow(event, sinceMs, untilMs));
      return { items, complete: true };
    },
  };
}

/** Drops one event when the caller paginates (splits the window into
 * two calls) despite claiming each page is complete — the suite must
 * reject this. */
export function createPageDroppingConnector(): ConnectorContract {
  return {
    registeredAt: REGISTERED_AT_MS,
    async fetchEvents(sinceMs, untilMs, filter) {
      const isPaginatedCall = sinceMs !== 0 || untilMs !== 5_000;
      let items = FIXTURE_EVENTS.filter((event) => inWindow(event, sinceMs, untilMs) && (!filter?.symbol || event.symbol === filter.symbol));
      if (isPaginatedCall) {
        items = items.filter((event) => event.id !== "evt-2");
      }
      return { items, complete: true };
    },
  };
}
