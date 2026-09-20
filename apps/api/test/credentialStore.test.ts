import { describe, expect, it } from "vitest";
import { MemoryCredentialStore } from "../src/auth/credentialStore.js";

const SUBJECT_KEY = "0x1111111111111111111111111111111111111111" as const;
const QX = "0x2222222222222222222222222222222222222222222222222222222222222222" as const;
const QY = "0x3333333333333333333333333333333333333333333333333333333333333333" as const;

describe("MemoryCredentialStore", () => {
  it("register stores a credential retrievable by get", async () => {
    const store = new MemoryCredentialStore();
    await store.register(SUBJECT_KEY, QX, QY, "webauthn");

    expect(await store.get(SUBJECT_KEY)).toEqual({
      subjectKey: SUBJECT_KEY,
      method: "webauthn",
      qx: QX,
      qy: QY,
      counter: 0,
    });
  });

  it("get returns undefined for a subjectKey that was never registered", async () => {
    const store = new MemoryCredentialStore();
    expect(await store.get(SUBJECT_KEY)).toBeUndefined();
  });
});
