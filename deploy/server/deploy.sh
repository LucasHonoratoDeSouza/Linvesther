#!/bin/bash
# Builds and switches to one commit, and goes back to the previous one if the
# new services do not come up healthy. Usage: deploy.sh <commit>
set -euo pipefail
ROOT="$HOME/linvesther"
REPO="$ROOT/repo"
target="$1"
export PATH="$HOME/.local/opt/node-v24.18.0-linux-x64/bin:$HOME/.cargo/bin:$HOME/.risc0/bin:$PATH"
exec 9>"$ROOT/shared/deploy.lock"
flock -n 9 || { echo "another deploy is running"; exit 0; }

previous="$(cat "$ROOT/shared/deployed" 2>/dev/null || true)"
previous_dist="$(cat "$ROOT/shared/dist-current" 2>/dev/null || echo .next-a)"
previous_worker="$(readlink "$ROOT/current-worker" 2>/dev/null || true)"

healthy() {
  for _ in $(seq 1 30); do
    if curl -fsS -o /dev/null http://127.0.0.1:4301/healthz && curl -fsS -o /dev/null http://127.0.0.1:4300/; then return 0; fi
    sleep 2
  done
  return 1
}

restart() { systemctl --user restart linvesther-api.service linvesther-web.service; }

rollback() {
  echo "deploy of $target failed; going back to ${previous:-nothing}" >&2
  echo "$target" > "$ROOT/shared/failed"
  [ -n "$previous" ] || return 0
  git -C "$REPO" checkout -q --detach "$previous"
  (cd "$REPO" && pnpm install --frozen-lockfile >/dev/null)
  echo "$previous_dist" > "$ROOT/shared/dist-current"
  [ -n "$previous_worker" ] && ln -sfn "$previous_worker" "$ROOT/current-worker"
  restart
}

git -C "$REPO" fetch -q origin
git -C "$REPO" checkout -q --detach "$target"
cd "$REPO"
pnpm install --frozen-lockfile

# The worker, built beside the running one.
(cd services && cargo build --release -p binance-worker)
cp services/target/release/binance-worker "$ROOT/releases/worker-$target"

# The web build goes into the spare directory while the site keeps serving the current one.
if [ "$previous_dist" = ".next-a" ]; then next=".next-b"; else next=".next-a"; fi
set -a; . "$REPO/deploy/server/settings.env"; set +a
rm -rf "apps/web/$next"
(cd apps/web && NEXT_DIST_DIR="$next" pnpm exec next build)
git checkout -- apps/web/tsconfig.json apps/web/next-env.d.ts 2>/dev/null || true

# Pick up changed unit files (a changed tunnel unit takes effect at its next restart).
"$REPO/deploy/server/install.sh" >/dev/null
ln -sfn "$ROOT/releases/worker-$target" "$ROOT/current-worker"
echo "$next" > "$ROOT/shared/dist-current"
restart
if healthy; then
  echo "$target" > "$ROOT/shared/deployed"
  rm -f "$ROOT/shared/failed"
  # Keep the last few worker binaries for rollback, drop the rest.
  ls -1t "$ROOT"/releases/worker-* | tail -n +4 | xargs -r rm -f
  echo "deployed $target"
else
  rollback
  exit 1
fi
