#!/usr/bin/env bash
# Create a user on the local test server: ./add_user.sh <name> <password> [--admin]
set -euo pipefail
admin=--no-admin; [[ "${3:-}" == "--admin" ]] && admin=--admin
docker exec mtrx-synapse register_new_matrix_user -c /config/homeserver.yaml \
  -u "$1" -p "$2" $admin http://localhost:8008
