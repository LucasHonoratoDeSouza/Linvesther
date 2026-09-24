import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// This server cannot write to Linvesther because the code that writes is
// not in it — not disabled at run time, absent. That is a property of
// what this package depends on and imports, so it is checked here rather
// than argued for in a comment.

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    return entry.isFile() && entry.name.endsWith(".ts") ? [path] : [];
  });
}

const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as {
  dependencies: Record<string, string>;
  devDependencies: Record<string, string>;
};

describe("what this server is built from", () => {
  it("does not depend on the API package, so no route that writes is installed with it", () => {
    const declared = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies });
    expect(declared.filter((name) => name.startsWith("@linvestherzk/"))).toEqual([]);
  });

  it("imports nothing from the API, the worker or any module that can change an account", () => {
    const forbidden = [
      "@linvestherzk/api",
      "binance-connect",
      "apps/api",
      "chain-worker",
      // Spawning a process is how the API reaches the Rust worker; this
      // server talks HTTP and has no reason to start anything.
      "node:child_process",
      "child_process",
      "node:worker_threads",
      "pg",
    ];
    for (const file of sourceFiles(join(root, "src"))) {
      const source = readFileSync(file, "utf8");
      for (const module of forbidden) {
        expect(source.includes(`from "${module}`), `${file} imports ${module}`).toBe(false);
        expect(source.includes(`import("${module}`), `${file} imports ${module}`).toBe(false);
        expect(source.includes(`require("${module}`), `${file} requires ${module}`).toBe(false);
      }
    }
  });

  it("only ever asks the API for a read: every request it can make is a GET under /mcp/account", () => {
    const api = readFileSync(join(root, "src", "api.ts"), "utf8");
    const methods = [...api.matchAll(/method:\s*"([A-Z]+)"/g)].map((match) => match[1]);
    expect(methods.length).toBeGreaterThan(0);
    expect(new Set(methods)).toEqual(new Set(["GET"]));

    const paths = [...api.matchAll(/this\.read\("([^"]+)"/g)].map((match) => match[1]);
    expect(paths).toHaveLength(6);
    expect(paths.every((path) => path?.startsWith("/mcp/account/"))).toBe(true);
  });
});
