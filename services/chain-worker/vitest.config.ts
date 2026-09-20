import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    testTimeout: 60_000,
    // Several suites each spawn a real local Anvil node on a fixed
    // port; running test files in parallel workers intermittently
    // starves those spawns under this environment's resource limits.
    // Sequential execution trades some wall-clock time for a gate that
    // doesn't flake.
    fileParallelism: false,
  },
});
