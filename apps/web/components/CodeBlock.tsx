"use client";

import { useState } from "react";
import { Icon } from "./Icon";

export function CodeBlock({ code, label }: { code: string; label: string }) {
  const [status, setStatus] = useState<"idle" | "copied" | "error">("idle");
  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setStatus("copied");
    } catch {
      setStatus("error");
    }
  }
  return (
    <div className="code-block">
      <span className="eyebrow">{label}</span>
      <button onClick={copy}>
        <Icon name={status === "copied" ? "check" : "copy"} width={13} />
        {status === "copied" ? "Copied" : "Copy"}
      </button>
      <pre>
        <code>{code}</code>
      </pre>
      <span role="status" className="field-hint">
        {status === "error"
          ? "Could not copy. Select and copy the code manually."
          : status === "copied"
            ? "Copied to clipboard."
            : null}
      </span>
    </div>
  );
}
