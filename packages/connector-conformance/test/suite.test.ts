import { describe, expect, it } from "vitest";
import { isFullyConformant, runConformanceSuite } from "../src/suite.js";
import { createBackdatingConnector, createConformantConnector, createFilterIgnoringConnector, createPageDroppingConnector } from "./syntheticConnector.js";

const WINDOW_START_MS = 0;
const WINDOW_END_MS = 5_000;

describe("Connector conformance suite", () => {
  it("a well-formed, independent synthetic connector passes every check", async () => {
    const report = await runConformanceSuite(createConformantConnector(), WINDOW_START_MS, WINDOW_END_MS);
    expect(report.failures).toEqual([]);
    expect(isFullyConformant(report)).toBe(true);
  });

  it("a connector returning backdated events is rejected", async () => {
    const report = await runConformanceSuite(createBackdatingConnector(), WINDOW_START_MS, WINDOW_END_MS);
    expect(isFullyConformant(report)).toBe(false);
    expect(report.failures.some((failure) => failure.check === "no_backdated_events")).toBe(true);
  });

  it("a connector that ignores a filter is rejected", async () => {
    const report = await runConformanceSuite(createFilterIgnoringConnector(), WINDOW_START_MS, WINDOW_END_MS);
    expect(isFullyConformant(report)).toBe(false);
    expect(report.failures.some((failure) => failure.check === "filter_honored")).toBe(true);
  });

  it("a connector that drops an event when paginated is rejected", async () => {
    const report = await runConformanceSuite(createPageDroppingConnector(), WINDOW_START_MS, WINDOW_END_MS);
    expect(isFullyConformant(report)).toBe(false);
    expect(report.failures.some((failure) => failure.check === "no_missing_page")).toBe(true);
  });
});
