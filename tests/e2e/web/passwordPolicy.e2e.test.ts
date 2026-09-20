import { describe, expect, it } from "vitest";
import { MIN_PASSWORD_LENGTH, passwordProblem } from "../../../apps/web/lib/passwordPolicy";

describe("choosing a password for a new identity", () => {
  it("refuses an empty or short password, and says why", () => {
    expect(passwordProblem("")).toMatch(/enter a password/i);
    expect(passwordProblem("a".repeat(MIN_PASSWORD_LENGTH - 1))).toMatch(/at least 10 characters/);
  });

  it("accepts a password of the minimum length, whatever it contains", () => {
    expect(passwordProblem("a".repeat(MIN_PASSWORD_LENGTH))).toBeNull();
    expect(passwordProblem("correct horse battery staple")).toBeNull();
  });

  it("counts characters a person sees, not bytes", () => {
    expect(passwordProblem("🔑".repeat(MIN_PASSWORD_LENGTH))).toBeNull();
    expect(passwordProblem("🔑".repeat(MIN_PASSWORD_LENGTH - 1))).not.toBeNull();
  });
});
