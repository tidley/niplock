#!/usr/bin/env bash
set -euo pipefail

# Required env vars:
# DEPLOY_HOST   e.g. 203.0.113.10 or server.example.com
# DEPLOY_USER   e.g. deploy
# DEPLOY_PATH   e.g. /var/www/nsyte.run
# Optional:
# DEPLOY_PORT   default 22

if [[ -z "${DEPLOY_HOST:-}" || -z "${DEPLOY_USER:-}" || -z "${DEPLOY_PATH:-}" ]]; then
  echo "Missing required env vars: DEPLOY_HOST, DEPLOY_USER, DEPLOY_PATH" >&2
  exit 1
fi

DEPLOY_PORT="${DEPLOY_PORT:-22}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

"$ROOT_DIR/scripts/build-web.sh"

rsync -az --delete -e "ssh -p $DEPLOY_PORT" "$ROOT_DIR/dist/" "$DEPLOY_USER@$DEPLOY_HOST:$DEPLOY_PATH/"

echo "Deployed dist/ to $DEPLOY_USER@$DEPLOY_HOST:$DEPLOY_PATH"
