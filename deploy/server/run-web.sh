#!/bin/sh
# Serves the production build named in shared/dist-current.
cd "$HOME/linvesther/repo/apps/web"
NEXT_DIST_DIR="$(cat "$HOME/linvesther/shared/dist-current")"
export NEXT_DIST_DIR
exec node node_modules/next/dist/bin/next start --port 4300 --hostname 127.0.0.1
