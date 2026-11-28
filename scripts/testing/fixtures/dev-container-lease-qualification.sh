#!/bin/sh
set -eu
umask 077
[ "$#" -eq 2 ] && [ "$1" = "--workspace-key" ] || { echo "Usage: $0 --workspace-key <owned-key>" >&2; exit 2; }
workspace_key=$2
printf %s "$workspace_key" | grep -Eq '^[a-z0-9]+(-[a-z0-9]+)*$' || { echo INVALID_WORKSPACE_KEY >&2; exit 2; }
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
socket="$HOME/.colima/erpx-opensource-dev/docker.sock"
[ -S "$socket" ] || { echo OWNED_ENGINE_REQUIRED >&2; exit 1; }
export DOCKER_HOST="unix://$socket"
cache="erpx-dev-${workspace_key}-node26.11.1-bun1.4.3-modules"
project="erpx-dev-${workspace_key}"
[ "$(docker volume inspect --format '{{ index .Labels "com.docker.compose.project" }}' "$cache")" = "$project" ] && [ "$(docker volume inspect --format '{{ index .Labels "com.docker.compose.volume" }}' "$cache")" = node-modules ] || { echo OWNED_VERSIONED_CACHE_REQUIRED >&2; exit 1; }
id=$(openssl rand -hex 8)
work="erpx-lease-check-$id-work"; pg="erpx-lease-check-$id-pg"; secrets="erpx-lease-check-$id-secrets"; data="erpx-lease-check-$id-data"
label="erpx.lease-qualification=$id"
cleanup() { docker rm -f "$pg" "$work" >/dev/null 2>&1 || true; docker volume rm "$data" "$secrets" >/dev/null 2>&1 || true; }
trap cleanup EXIT HUP INT TERM
docker volume create --label "$label" "$secrets" >/dev/null
docker volume create --label "$label" "$data" >/dev/null
docker run --rm --network none --label "$label" -v "$secrets:/run/erpx-secrets" erpx-opensource-dev-toolchain:local sh -ec 'umask 077; openssl rand -hex 32 > /run/erpx-secrets/postgres; openssl rand -hex 32 > /run/erpx-secrets/test-admin'
docker run -d --network none --label "$label" --name "$work" -v "$secrets:/run/erpx-secrets:ro" -v "$cache:/case/node_modules:ro" erpx-opensource-dev-toolchain:local sleep infinity >/dev/null
docker run -d --network "container:$work" --label "$label" --name "$pg" -v "$secrets:/run/erpx-secrets:ro" -v "$data:/var/lib/postgresql" -e POSTGRES_PASSWORD_FILE=/run/erpx-secrets/postgres 'postgres:18.6-trixie@sha256:979bccfde35445db618c65107d27d08c6c21339d8dc3ad4c830c6633947be0e4' >/dev/null
docker exec "$work" mkdir -p /case/scripts/local /case/scripts/testing/fixtures
docker cp "$root/scripts/local/dev-container-native.ts" "$work:/case/scripts/local/dev-container-native.ts"
docker cp "$root/scripts/local/dev-container-verify-fk.ts" "$work:/case/scripts/local/dev-container-verify-fk.ts"
docker cp "$root/scripts/testing/fixtures/dev-container-lease-qualification.ts" "$work:/case/scripts/testing/fixtures/dev-container-lease-qualification.ts"
printf 'Helper before: '; shasum -a 256 "$root/scripts/local/dev-container-native.ts" | awk '{print $1}'
docker exec -w /case "$work" bun --no-env-file scripts/testing/fixtures/dev-container-lease-qualification.ts
printf 'Helper after: '; shasum -a 256 "$root/scripts/local/dev-container-native.ts" | awk '{print $1}'
printf 'Cleanup scoped to label %s\n' "$label"
