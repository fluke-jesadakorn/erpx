#!/bin/sh
# This command touches only the explicitly named Colima profile and project.
set -eu
case "${ERPX_DEV_WORKSPACE_KEY:-}" in
  ''|*[!a-z0-9-]*|-*|*-) echo 'Set ERPX_DEV_WORKSPACE_KEY to a unique lowercase checkout-and-architecture key.' >&2; exit 2 ;;
esac
socket="$HOME/.colima/erpx-opensource-dev/docker.sock"
[ -S "$socket" ] || { echo 'OWNED_CONTAINER_ENGINE_REQUIRED' >&2; exit 1; }
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
unset DOCKER_CONTEXT DOCKER_TLS_VERIFY DOCKER_CERT_PATH
. "$root/scripts/local/dev-container-lifecycle-lock.sh"
acquire_lifecycle_lock
DOCKER_CONFIG=
trap '[ -z "$DOCKER_CONFIG" ] || rm -rf -- "$DOCKER_CONFIG"; release_lifecycle_lock' EXIT
DOCKER_CONFIG=$(mktemp -d "${TMPDIR:-/tmp}/erpx-container-cli.XXXXXXXX")
trap 'exit 1' HUP INT TERM
export DOCKER_CONFIG COMPOSE_DISABLE_ENV_FILE=1 DOCKER_HOST="unix://$socket"
project="erpx-dev-$ERPX_DEV_WORKSPACE_KEY"
services_workspace_mode=false
if [ "${1:-}" = up ] && [ "${2:-}" = --workspace ] && [ "$#" -eq 2 ]; then services_workspace_mode=true; fi
. "$root/scripts/local/dev-container-services-lib.sh"
case "${1:-}" in
  up) [ "$#" -eq 1 ] || [ "$services_workspace_mode" = true ] || exit 2; services_up ;;
  down) [ "$#" -eq 1 ] || exit 2; services_compose down ;;
  *) echo 'Usage: ERPX_DEV_WORKSPACE_KEY=<checkout-architecture> dev-container-services.sh up [--workspace] | down' >&2; exit 2 ;;
esac
