"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { AppShell } from "../../components/AppShell";
import { Icon } from "../../components/Icon";

const ADDRESS = /^0x[0-9a-fA-F]{40}$/;

/** What a search box entry points at: a person's public address (or a link
 * to their profile), or a protocol Track ID (or a link to one). */
function resolveTarget(raw: string): { path: string } | { error: string } {
  const text = raw.trim();
  if (ADDRESS.test(text)) return { path: `/p/${text}` };

  let candidate = text;
  if (text.includes("://")) {
    let url: URL;
    try {
      url = new URL(text);
    } catch {
      return { error: "That doesn’t look like a link. Paste an address (0x…) or a profile link." };
    }
    if (!["https:", "http:"].includes(url.protocol)) {
      return { error: "Enter a public profile link starting with http or https." };
    }
    const person = url.pathname.match(/^\/p\/(0x[0-9a-fA-F]{40})\/?$/);
    if (person) return { path: `/p/${person[1]}` };
    const track = url.pathname.match(/^\/profile\/([^/]+)\/?$/);
    if (!track) return { error: "Enter a link that ends in /p/0x… (a person) or /profile/… (a track)." };
    candidate = decodeURIComponent(track[1]!);
  }
  if (!/^[a-zA-Z0-9_-]{1,160}$/.test(candidate)) {
    return { error: "Enter an address (0x…), a profile link, or a Track ID with letters, numbers, hyphens or underscores." };
  }
  return { path: `/profile/${encodeURIComponent(candidate)}` };
}

export default function ExplorerPage() {
  const router = useRouter();
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);

  function search(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const target = resolveTarget(query);
    if ("error" in target) {
      setError(target.error);
      return;
    }
    router.push(target.path);
  }

  return (
    <AppShell>
      <main className="shell">
        <div className="hero">
          <div className="hero-label">Public explorer</div>
          <h1 className="hero-title">Look into the evidence.</h1>
          <p className="hero-subtitle">
            Look up anyone’s public track record with their address or profile link. You only see what its owner chose to
            share — no sign-in needed.
          </p>
        </div>
        <div className="explorer-search">
          <label className="field-label" htmlFor="track-search">
            Address or public profile link
          </label>
          <form onSubmit={search}>
            <Icon name="search" />
            <input
              id="track-search"
              data-testid="track-search-input"
              placeholder="Paste an address (0x…) or a profile link"
              value={query}
              maxLength={2048}
              onChange={(event) => {
                setQuery(event.target.value);
                setError(null);
              }}
              aria-describedby="search-help"
            />
            <button className="button button-accent" data-testid="track-search-button" disabled={!query.trim()}>
              Look up <Icon name="arrow" width={16} />
            </button>
          </form>
          <p className="field-hint" id="search-help">
            Public information only. A protocol Track ID works too.
          </p>
        </div>
        {error && (
          <div className="error-banner" role="alert">
            {error}
          </div>
        )}
        <div className="explorer-empty">
          <Icon name="globe" width={36} height={36} />
          <h2>How a public profile works.</h2>
          <p>
            The owner connects an account with a read-only key, picks which numbers to publish — return, worst drop,
            risk-adjusted return, winning trades — and signs it. What you see here is exactly that: never balances,
            positions or trades, and always counted from when the account was connected.
          </p>
          <a className="text-link" href="/docs/concepts#verification">
            Learn to read a track <Icon name="diagonal" width={16} />
          </a>
        </div>
      </main>
    </AppShell>
  );
}
