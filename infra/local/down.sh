#!/usr/bin/env bash
# Stops the local chain. Its state is saved, so `up.sh` resumes where it left off.
set -euo pipefail
STATE_DIR="$(cd "$(dirname "$0")" && pwd)/.state"
if [[ -f "$STATE_DIR/anvil.pid" ]]; then
  pid="$(cat "$STATE_DIR/anvil.pid")"
  # SIGINT lets Anvil write its state file before it exits.
  kill -INT "$pid" 2>/dev/null || true
  for _ in $(seq 1 50); do kill -0 "$pid" 2>/dev/null || break; sleep 0.1; done
  rm -f "$STATE_DIR/anvil.pid"
fi
echo "local chain stopped (the database container keeps running; stop it with: docker compose -f infra/docker-compose.yml stop postgres)"
