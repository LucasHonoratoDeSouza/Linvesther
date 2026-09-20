import { describe, expect, it } from "vitest";
import { hostRewrites } from "../../../apps/web/lib/hostRewrites.mjs";

// The apex, `app.` and `docs.` hosts share one deployment and show a different
// page at "/". A middleware rewrite for this returned a 500 behind a
// TLS-terminating proxy, so it is declared as a config rewrite instead.
describe("what each host shows at the front page", () => {
  it("sends the app host to the portfolio and the docs host to the docs", () => {
    expect(hostRewrites("example.test")).toEqual({
      beforeFiles: [
        { source: "/", has: [{ type: "host", value: "app.example.test" }], destination: "/portfolio" },
        { source: "/", has: [{ type: "host", value: "docs.example.test" }], destination: "/docs" },
      ],
    });
  });

  it("changes nothing when there is no root domain (local development, access by IP)", () => {
    expect(hostRewrites(undefined)).toEqual([]);
    expect(hostRewrites("")).toEqual([]);
  });
});
