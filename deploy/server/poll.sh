#!/bin/bash
# Deploys the newest commit of DEPLOY_BRANCH once every CI job for it has passed.
# The server pulls; nothing outside can reach it, and no secret sits on GitHub.
set -euo pipefail
ROOT="$HOME/linvesther"
REPO_SLUG="${REPO_SLUG:-LucasHonoratoDeSouza/Linvesther}"
BRANCH="${DEPLOY_BRANCH:-$(cat "$ROOT/shared/deploy-branch" 2>/dev/null || echo main)}"

git -C "$ROOT/repo" fetch -q origin "$BRANCH"
sha="$(git -C "$ROOT/repo" rev-parse "origin/$BRANCH")"
[ "$sha" = "$(cat "$ROOT/shared/deployed" 2>/dev/null || true)" ] && exit 0
# A commit that already failed to deploy is not retried until a newer one exists.
[ "$sha" = "$(cat "$ROOT/shared/failed" 2>/dev/null || true)" ] && exit 0

runs="$(curl -fsS "https://api.github.com/repos/$REPO_SLUG/commits/$sha/check-runs?per_page=100")"
verdict="$(printf '%s' "$runs" | python3 -c '
import json, sys
runs = json.load(sys.stdin)["check_runs"]
if not runs: print("wait")
elif any(r["status"] != "completed" for r in runs): print("wait")
elif all(r["conclusion"] in ("success", "skipped", "neutral") for r in runs): print("ok")
else: print("red")
')"
case "$verdict" in
  ok) exec "$ROOT/repo/deploy/server/deploy.sh" "$sha" ;;
  red) echo "$sha" > "$ROOT/shared/failed"; echo "CI is red for $sha; not deploying" >&2 ;;
  *) echo "CI for $sha has not finished" ;;
esac
