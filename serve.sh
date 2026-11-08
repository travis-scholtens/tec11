#!/usr/bin/env bash
set -euo pipefail
PORT="${1:-8080}"
echo "Serving Tec11 on http://127.0.0.1:${PORT} ..."
exec python3 -m http.server "${PORT}" --bind 127.0.0.1 --directory dist
