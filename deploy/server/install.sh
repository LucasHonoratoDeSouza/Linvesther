#!/bin/bash
# Installs the per-user services and the deploy timer. Run once on the server.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$HOME/.config/systemd/user"
cp "$here"/systemd/*.service "$here"/systemd/*.timer "$HOME/.config/systemd/user/"
systemctl --user daemon-reload
systemctl --user enable linvesther-api.service linvesther-web.service linvesther-tunnel.service linvesther-deploy.timer
