import { describe, expect, it } from "vitest";
import { contentSecurityPolicy } from "../../../apps/web/lib/contentSecurityPolicy";

const policy = (development = false, apiUrl = "https://api.example.test/some/path") => contentSecurityPolicy({ nonce: "abc123", apiUrl, development });
const directive = (text: string, name: string) => text.split("; ").find((part) => part.startsWith(`${name} `) || part === name);

describe("the page's content security policy", () => {
  it("lets a script run only if it carries this request's nonce", () => {
    const scripts = directive(policy(), "script-src")!;
    expect(scripts).toContain("'nonce-abc123'");
    expect(scripts).not.toContain("'unsafe-inline'");
    expect(scripts).not.toContain("'unsafe-eval'");
  });

  it("allows eval only while developing", () => {
    expect(directive(policy(true), "script-src")).toContain("'unsafe-eval'");
  });

  it("lets the page talk to itself and to its API, and nowhere else", () => {
    expect(directive(policy(), "connect-src")).toBe("connect-src 'self' https://api.example.test");
    expect(directive(policy(false, "not a url"), "connect-src")).toBe("connect-src 'self'");
  });

  it("forbids framing, plugins, a changed base address and foreign forms", () => {
    const text = policy();
    expect(directive(text, "frame-ancestors")).toBe("frame-ancestors 'none'");
    expect(directive(text, "object-src")).toBe("object-src 'none'");
    expect(directive(text, "base-uri")).toBe("base-uri 'self'");
    expect(directive(text, "form-action")).toBe("form-action 'self'");
  });

  it("upgrades insecure requests in production only", () => {
    expect(policy()).toContain("upgrade-insecure-requests");
    expect(policy(true)).not.toContain("upgrade-insecure-requests");
  });
});
