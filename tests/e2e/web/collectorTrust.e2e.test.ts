import { describe, expect, it } from "vitest";
import { assessOrigin } from "../../../apps/web/lib/collectorTrust";

const FINGERPRINT = "9c177b47de4c524a7b7754c648e488d3993b12b88930e0d67f9e3bcd01afc134";
const NOW = Date.parse("2026-09-20T00:00:00Z");
const listed = [{ name: "example collector", fingerprint: FINGERPRINT.toUpperCase() }];

describe("the origin label a public page shows", () => {
  it("names a collector that is on the list", () => {
    expect(assessOrigin({ mechanism: "A0", collector: FINGERPRINT }, NOW, listed)).toEqual({ kind: "trusted", name: "example collector" });
  });

  it("calls a collector that is not on the list self-attested, never trusted", () => {
    expect(assessOrigin({ mechanism: "A0", collector: FINGERPRINT }, NOW, [])).toEqual({ kind: "self-attested", fingerprint: FINGERPRINT });
  });

  it("does not trust a revoked collector, or one outside its dates", () => {
    expect(assessOrigin({ mechanism: "A0", collector: FINGERPRINT }, NOW, [{ ...listed[0]!, revoked: true }]).kind).toBe("self-attested");
    expect(assessOrigin({ mechanism: "A0", collector: FINGERPRINT }, NOW, [{ ...listed[0]!, validUntilMs: NOW - 1 }]).kind).toBe("self-attested");
    expect(assessOrigin({ mechanism: "A0", collector: FINGERPRINT }, NOW, [{ ...listed[0]!, validFromMs: NOW + 1 }]).kind).toBe("self-attested");
  });

  it("says the source is unknown when the instance names no collector", () => {
    expect(assessOrigin({ mechanism: "A0", collector: null }, NOW, listed).kind).toBe("unknown");
    expect(assessOrigin(undefined, NOW, listed).kind).toBe("unknown");
  });

  it("ships an empty list until a collector is deliberately added", () => {
    expect(assessOrigin({ mechanism: "A0", collector: FINGERPRINT }).kind).toBe("self-attested");
  });
});
