import type { ConnectorContract } from "./contract.js";

export interface ConformanceFailure {
  check: string;
  reason: string;
}

export interface ConformanceReport {
  passed: string[];
  failures: ConformanceFailure[];
}

function ok(report: ConformanceReport, check: string): void {
  report.passed.push(check);
}

function fail(report: ConformanceReport, check: string, reason: string): void {
  report.failures.push({ check, reason });
}

/** Runs the conformance battery against any `ConnectorContract`
 * implementation. Per the acceptance criteria: a well-formed, independent
 * synthetic connector passes; one that backdates, ignores a filter, or
 * omits a page is rejected — never silently accepted as complete. */
export async function runConformanceSuite(connector: ConnectorContract, windowStartMs: number, windowEndMs: number): Promise<ConformanceReport> {
  const report: ConformanceReport = { passed: [], failures: [] };

  const fullWindow = await connector.fetchEvents(windowStartMs, windowEndMs);
  if (fullWindow.complete) {
    ok(report, "full_window_complete");
  } else {
    fail(report, "full_window_complete", "connector reported the full requested window as incomplete");
  }

  const backdated = fullWindow.items.filter((event) => event.economicTimeMs < connector.registeredAt);
  if (backdated.length === 0) {
    ok(report, "no_backdated_events");
  } else {
    fail(report, "no_backdated_events", `${backdated.length} event(s) with economicTimeMs before registeredAt were returned as eligible history`);
  }

  const symbols = [...new Set(fullWindow.items.map((event) => event.symbol))];
  if (symbols.length > 0) {
    const targetSymbol = symbols[0]!;
    const filtered = await connector.fetchEvents(windowStartMs, windowEndMs, { symbol: targetSymbol });
    const allMatchFilter = filtered.items.every((event) => event.symbol === targetSymbol);
    if (allMatchFilter) {
      ok(report, "filter_honored");
    } else {
      fail(report, "filter_honored", `filtering by symbol=${targetSymbol} returned event(s) for a different symbol`);
    }
  } else {
    ok(report, "filter_honored");
  }

  // Missing-page check: fetching each half of the window separately
  // must account for the same events as fetching the full window in
  // one call — a connector that silently drops a page when the caller
  // paginates would diverge here.
  const midpoint = windowStartMs + Math.floor((windowEndMs - windowStartMs) / 2);
  const firstHalf = await connector.fetchEvents(windowStartMs, midpoint);
  const secondHalf = await connector.fetchEvents(midpoint, windowEndMs);
  const pagedIds = new Set([...firstHalf.items, ...secondHalf.items].map((event) => event.id));
  const fullIds = new Set(fullWindow.items.map((event) => event.id));
  const missing = [...fullIds].filter((id) => !pagedIds.has(id));
  if (firstHalf.complete && secondHalf.complete && missing.length === 0) {
    ok(report, "no_missing_page");
  } else {
    fail(report, "no_missing_page", `paginated fetch is missing ${missing.length} event(s) present in the full-window fetch, or reported a page as incomplete`);
  }

  return report;
}

export function isFullyConformant(report: ConformanceReport): boolean {
  return report.failures.length === 0;
}
