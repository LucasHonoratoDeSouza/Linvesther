import { describe, expect, it } from "vitest";
import { hostContext } from "../../../apps/web/lib/hostContext";

describe("what the header offers, by host", () => {
  it("offers the website to someone already in the app", () => {
    expect(hostContext("app.example.test", "example.test", "https")).toEqual({ onAppHost: true, websiteUrl: "https://example.test" });
    expect(hostContext("APP.example.test:443", "example.test", "https").onAppHost).toBe(true);
  });

  it("offers the app everywhere else", () => {
    for (const host of ["example.test", "docs.example.test", "evil-app.example.test", "app.example.test.evil.test", null]) {
      expect(hostContext(host, "example.test", "https").onAppHost, String(host)).toBe(false);
    }
  });

  it("changes nothing without a root domain (local development)", () => {
    expect(hostContext("app.localhost", undefined, null)).toEqual({ onAppHost: false, websiteUrl: "/" });
  });

  it("keeps http for a plain-http deployment", () => {
    expect(hostContext("app.example.test", "example.test", "http").websiteUrl).toBe("http://example.test");
  });
});
