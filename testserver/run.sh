#!/usr/bin/env bash
# Start (or restart) the local Synapse test server on http://localhost:8008.
#   ./run.sh          start, keeping existing data
#   ./run.sh --reset  wipe data first
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
name=mtrx-synapse
image=matrixdotorg/synapse:latest

if [[ "${1:-}" == "--reset" ]]; then rm -rf "$here/data"; fi
mkdir -p "$here/data"
docker rm -f "$name" >/dev/null 2>&1 || true

docker run -d --name "$name" -p 8008:8008 \
  -v "$here/data:/data" -v "$here:/config:ro" \
  -e SYNAPSE_CONFIG_PATH=/config/homeserver.yaml \
  "$image" >/dev/null

for _ in $(seq 1 60); do
  if curl -fs http://localhost:8008/health >/dev/null; then echo "synapse up"; exit 0; fi
  sleep 1
done
echo "synapse failed to start" >&2; docker logs "$name" | tail -20 >&2; exit 1
