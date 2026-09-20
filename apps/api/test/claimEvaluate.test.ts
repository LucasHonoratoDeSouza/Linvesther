import { describe, expect, it } from "vitest";
import { checkClaims } from "../src/claims/evaluate.js";

describe("checkClaims", () => {
  const metrics = { return: 0.12, maxDrawdown: 0.08 };

  it("accepts claims that hold", () => {
    expect(checkClaims([{ type: "GE", metric: "return", threshold: "0.10" }], metrics)).toEqual({ ok: true });
    expect(checkClaims([{ type: "LE", metric: "maxDrawdown", threshold: "0.10" }], metrics)).toEqual({ ok: true });
  });

  it("rejects a claim that does not hold", () => {
    expect(checkClaims([{ type: "GE", metric: "return", threshold: "0.20" }], metrics)).toEqual({ ok: false, error: "claim_not_satisfied" });
  });

  it("rejects a metric that is not available yet", () => {
    expect(checkClaims([{ type: "GE", metric: "sharpe", threshold: "1" }], metrics)).toEqual({ ok: false, error: "metric_unavailable" });
  });

  it("rejects empty and unsupported claims", () => {
    expect(checkClaims([], metrics)).toEqual({ ok: false, error: "unsupported_claim" });
    expect(checkClaims([{ type: "VALUE", metric: "return", value: "0.12" }], metrics)).toEqual({ ok: false, error: "unsupported_claim" });
  });
});
