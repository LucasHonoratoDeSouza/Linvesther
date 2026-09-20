// Invokes services/collector/binance/worker's Rust binary as a
// subprocess. Credentials go in via stdin only — never as a CLI
// argument (arguments are visible in the process list to anyone on
// the same machine) and never logged.

import { spawn } from "node:child_process";

export interface WorkerBinaryOptions {
  /** Path to the compiled binance-worker binary. */
  binaryPath: string;
}

export class WorkerInvocationError extends Error {
  constructor(
    message: string,
    public readonly stderr: string,
  ) {
    super(message);
    this.name = "WorkerInvocationError";
  }
}

/** Thrown instead of starting one more process when too many are already
 * running: each call spawns a real process, so an unbounded queue is a way
 * to exhaust the machine. Callers report it like any other worker failure. */
export class WorkerBusyError extends WorkerInvocationError {
  constructor() {
    super("the service is busy, try again in a moment", "");
    this.name = "WorkerBusyError";
  }
}

const MAX_CONCURRENT_WORKERS = Number(process.env.WORKER_MAX_CONCURRENCY ?? 8);
let runningWorkers = 0;

/** Runs `binaryPath <subcommand>`, writing `stdinPayload` (already
 * JSON-stringified) to its stdin, and parses its stdout as JSON. The
 * worker's own `{"ok": false, "error": ...}` shape on failure is
 * surfaced as a rejected promise, not swallowed. */
export function invokeWorker<T>(options: WorkerBinaryOptions, subcommand: "connect" | "status" | "list" | "rename" | "sync" | "series" | "nav" | "performance" | "prove-performance" | "collector-identity", stdinPayload: string): Promise<T> {
  if (runningWorkers >= MAX_CONCURRENT_WORKERS) {
    return Promise.reject(new WorkerBusyError());
  }
  runningWorkers += 1;
  return new Promise<T>((resolve, reject) => {
    const child = spawn(options.binaryPath, [subcommand], { stdio: ["pipe", "pipe", "pipe"] });

    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });

    let released = false;
    const release = () => {
      if (!released) {
        released = true;
        runningWorkers -= 1;
      }
    };
    child.on("error", (error) => {
      release();
      reject(new WorkerInvocationError(`failed to spawn worker binary: ${error.message}`, stderr));
    });

    child.on("close", () => {
      release();
      let parsed: unknown;
      try {
        parsed = JSON.parse(stdout);
      } catch {
        reject(new WorkerInvocationError("worker binary did not print valid JSON", stderr));
        return;
      }
      const result = parsed as { ok: boolean; error?: string };
      if (!result.ok) {
        reject(new WorkerInvocationError(result.error ?? "worker binary reported failure", stderr));
        return;
      }
      resolve(parsed as T);
    });

    child.stdin.write(stdinPayload);
    child.stdin.end();
  });
}
