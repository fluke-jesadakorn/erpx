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
DOCKER_CONFIG=$(mktemp -d "${TMPDIR:-/tmp}/erpx-container-cli.XXXXXXXX")
trap 'rm -rf -- "$DOCKER_CONFIG"' EXIT HUP INT TERM
export DOCKER_CONFIG COMPOSE_DISABLE_ENV_FILE=1
export DOCKER_HOST="unix://$socket"
project="erpx-dev-$ERPX_DEV_WORKSPACE_KEY"
compose() { docker-compose --project-directory "$root/ops/containers" -p "$project" -f "$root/ops/containers/development-services.compose.yaml" "$@"; }
case "${1:-}" in
  up)
    [ "$#" -eq 1 ] || exit 2
    # Leave headroom for the pinned service image pulls and container volumes.
    free_kib=$(df -Pk "$root" | awk 'NR==2 {print $4}')
    case "$free_kib" in ''|*[!0-9]*) echo 'HOST_DISK_SPACE_UNAVAILABLE' >&2; exit 1;; esac
    [ "$free_kib" -ge 65011712 ] || { echo 'HOST_DISK_SPACE_BELOW_SERVICE_HEADROOM' >&2; exit 1; }
    # The named volume contains only locally generated disposable credentials.
    if docker volume inspect "${project}_secrets" >/dev/null 2>&1; then
      docker run --rm --network=none -e ERPX_DEV_WORKSPACE_KEY -v "${project}_secrets:/secrets:ro" erpx-opensource-dev-toolchain:local sh -ec 'test -f /secrets/workspace-key && test "$(cat /secrets/workspace-key)" = "$ERPX_DEV_WORKSPACE_KEY" && test "$(stat -c %a /secrets/workspace-key)" = 600 && test -s /secrets/postgres && test -s /secrets/test-admin' || { echo WORKSPACE_LEASE_MISMATCH >&2; exit 1; }
    else
      # Never adopt an existing database volume when its secret lease is absent.
      if docker volume inspect "${project}-postgres" >/dev/null 2>&1 || docker volume inspect "${project}-source" >/dev/null 2>&1; then echo MISSING_LEASE_WITH_EXISTING_DATA >&2; exit 1; fi
      docker volume create "${project}_secrets" >/dev/null
      docker run --rm --network=none -e ERPX_DEV_WORKSPACE_KEY -v "${project}_secrets:/secrets" erpx-opensource-dev-toolchain:local sh -ec 'umask 077; printf "%s\n" "$ERPX_DEV_WORKSPACE_KEY" > /secrets/workspace-key'
    fi
    docker run --rm --network=none -v "${project}_secrets:/secrets" erpx-opensource-dev-toolchain:local sh -ec 'umask 077; test -s /secrets/postgres || openssl rand -hex 32 > /secrets/postgres; test -s /secrets/test-admin || openssl rand -hex 32 > /secrets/test-admin; printf "postgresql://erpx_test_admin:%s@127.0.0.1:5432/postgres" "$(cat /secrets/test-admin)" > /secrets/test-admin-url'
    compose up -d
    compose exec -T postgres sh -ec '
      tries=0
      until pg_isready -h 127.0.0.1 -U postgres -d postgres >/dev/null; do
        tries=$((tries+1)); [ "$tries" -lt 60 ] || { echo POSTGRES_NOT_READY >&2; exit 1; }; sleep 1
      done
      export PGPASSWORD="$(cat /run/erpx-secrets/postgres)"
      psql -X -q -v ON_ERROR_STOP=1 -h 127.0.0.1 -U postgres -d postgres -v test_password="$(cat /run/erpx-secrets/test-admin)" >/dev/null <<'"'"'SQL'"'"'
SELECT format('"'"'CREATE ROLE erpx_test_admin LOGIN CREATEDB PASSWORD %L'"'"', :'"'"'test_password'"'"') WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname='"'"'erpx_test_admin'"'"') \gexec
SQL
      psql -X -At -v ON_ERROR_STOP=1 -h 127.0.0.1 -U postgres -d postgres -c "SELECT rolcanlogin AND rolcreatedb AND NOT rolsuper AND NOT rolcreaterole FROM pg_roles WHERE rolname='"'"'erpx_test_admin'"'"'" | grep -qx t
      PGPASSWORD="$(cat /run/erpx-secrets/test-admin)" psql -X -At -v ON_ERROR_STOP=1 -h 127.0.0.1 -U erpx_test_admin -d postgres -c "SELECT current_user" | grep -qx erpx_test_admin
    '
    ;;
  down)
    [ "$#" -eq 1 ] || exit 2
    compose down
    ;;
  *) echo 'Usage: ERPX_DEV_WORKSPACE_KEY=<checkout-architecture> dev-container-services.sh up|down' >&2; exit 2 ;;
esac
