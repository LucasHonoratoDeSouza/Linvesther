import { describe, expect, it } from "vitest";
import { canonicalRedirect } from "../../../apps/web/lib/canonicalHost";

const ROOT = "linvesther.com";

describe("canonical address", () => {
  it("sends www to the bare domain over https, keeping path and query", () => {
    expect(canonicalRedirect("www.linvesther.com", "https", ROOT, "/a?b=1")).toBe(
      "https://linvesther.com/a?b=1",
    );
  });

  it("sends plain http to https on the same host", () => {
    expect(canonicalRedirect("docs.linvesther.com", "http", ROOT, "/docs")).toBe(
      "https://docs.linvesther.com/docs",
    );
    expect(canonicalRedirect("linvesther.com", "http", ROOT, "/")).toBe("https://linvesther.com/");
  });

  it("sends www over http straight to the bare https address", () => {
    expect(canonicalRedirect("www.linvesther.com", "http", ROOT, "/")).toBe("https://linvesther.com/");
  });

  it("leaves a canonical request alone", () => {
    expect(canonicalRedirect("linvesther.com", "https", ROOT, "/")).toBeNull();
    expect(canonicalRedirect("app.linvesther.com", null, ROOT, "/")).toBeNull();
  });

  it("does nothing without a root domain or for hosts that are not ours", () => {
    expect(canonicalRedirect("www.linvesther.com", "http", undefined, "/")).toBeNull();
    expect(canonicalRedirect("evil.example", "http", ROOT, "/")).toBeNull();
    expect(canonicalRedirect("notlinvesther.com", "http", ROOT, "/")).toBeNull();
  });
});
