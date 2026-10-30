#!/bin/sh
# Shared implementation; callers must hold the keyed host lifecycle lock.
services_compose() {
  if [ "$services_workspace_mode" = true ]; then
    docker-compose --project-directory "$root/ops/containers" -p "$project" -f "$root/ops/containers/development-services.compose.yaml" -f "$root/.devcontainer/compose.yaml" "$@"
  else
    docker-compose --project-directory "$root/ops/containers" -p "$project" -f "$root/ops/containers/development-services.compose.yaml" "$@"
  fi
}
services_up() {
    tool_image=erpx-opensource-dev-toolchain:local
    if [ "$services_workspace_mode" = true ]; then tool_image=erpx-opensource-dev-toolchain-files-lcms2191:local; fi
    # Stream only the pinned toolchain recipe and its source manifest; never send
    # the checkout or its ignored data as a Docker build context.
    if ! docker image inspect "$tool_image" >/dev/null 2>&1; then
      context=$(mktemp -d "${TMPDIR:-/tmp}/erpx-contributor-build.XXXXXXXX") || exit 1
      mkdir -p "$context/scripts/testing" "$context/fixtures/files/native-toolchain"
      cp "$root/.devcontainer/Dockerfile" "$context/Dockerfile"
      for file in provision-native-worker.ts native-worker-toolchain.ts native-worker-link-flags.ts cargo-build-profile.ts; do cp "$root/scripts/testing/$file" "$context/scripts/testing/$file"; done
      cp "$root/fixtures/files/native-toolchain/sources.json" "$context/fixtures/files/native-toolchain/sources.json"
      if ! COPYFILE_DISABLE=1 docker build -t "$tool_image" "$context"; then rm -rf "$context"; exit 1; fi
      rm -rf "$context"
    fi
    # Reject orphaned persistent stores before any listener check or creation.
    # Repeat the check at the creation boundary to cover concurrent changes.
    if ! docker volume inspect "${project}_secrets" >/dev/null 2>&1; then
      if docker volume inspect "${project}-native-files" >/dev/null 2>&1 || docker volume inspect "${project}-postgres" >/dev/null 2>&1 || docker volume inspect "${project}-source" >/dev/null 2>&1 || docker volume inspect "${project}-local-ca" >/dev/null 2>&1; then echo MISSING_LEASE_WITH_EXISTING_DATA >&2; exit 1; fi
    fi
    if [ "$services_workspace_mode" = true ]; then
      # Never adopt or stop another checkout's HTTPS listener. Docker also
      # performs an atomic bind, so a race after this check still fails closed.
      command -v lsof >/dev/null 2>&1 || { echo PORT_INSPECTION_TOOL_REQUIRED >&2; exit 1; }
      listeners=$(lsof -nP -tiTCP:3443 -sTCP:LISTEN 2>/dev/null || true)
      if [ -n "$listeners" ]; then
        owned=$(services_compose ps -q workspace)
        [ -n "$owned" ] && [ "$(docker inspect --format '{{.State.Running}}' "$owned")" = true ] && \
          [ "$(docker inspect --format '{{(index (index .NetworkSettings.Ports "3443/tcp") 0).HostIp}}' "$owned")" = 127.0.0.1 ] || { echo DEVELOPMENT_HTTPS_PORT_OCCUPIED:3443 >&2; exit 1; }
      fi
    fi
    # Leave headroom for the pinned service image pulls and container volumes.
    free_kib=$(df -Pk "$root" | awk 'NR==2 {print $4}')
    case "$free_kib" in ''|*[!0-9]*) echo 'HOST_DISK_SPACE_UNAVAILABLE' >&2; exit 1;; esac
    [ "$free_kib" -ge 65011712 ] || { echo 'HOST_DISK_SPACE_BELOW_SERVICE_HEADROOM' >&2; exit 1; }
    # The named volume contains only locally generated disposable credentials.
    if docker volume inspect "${project}_secrets" >/dev/null 2>&1; then
      docker run --rm --network=none -e ERPX_DEV_WORKSPACE_KEY -v "${project}_secrets:/secrets:ro" "$tool_image" sh -ec 'test -f /secrets/workspace-key && test "$(cat /secrets/workspace-key)" = "$ERPX_DEV_WORKSPACE_KEY" && test "$(stat -c %a /secrets/workspace-key)" = 600 && test -s /secrets/postgres && test -s /secrets/test-admin' || { echo WORKSPACE_LEASE_MISMATCH >&2; exit 1; }
    else
      # Never adopt an existing database volume when its secret lease is absent.
      if docker volume inspect "${project}-native-files" >/dev/null 2>&1 || docker volume inspect "${project}-postgres" >/dev/null 2>&1 || docker volume inspect "${project}-source" >/dev/null 2>&1 || docker volume inspect "${project}-local-ca" >/dev/null 2>&1; then echo MISSING_LEASE_WITH_EXISTING_DATA >&2; exit 1; fi
      docker volume create "${project}_secrets" >/dev/null
      docker run --rm --network=none -e ERPX_DEV_WORKSPACE_KEY -v "${project}_secrets:/secrets" "$tool_image" sh -ec 'umask 077; printf "%s\n" "$ERPX_DEV_WORKSPACE_KEY" > /secrets/workspace-key'
    fi
    docker run --rm --network=none -v "${project}_secrets:/secrets" "$tool_image" sh -ec 'umask 077; test -s /secrets/postgres || openssl rand -hex 32 > /secrets/postgres; test -s /secrets/test-admin || openssl rand -hex 32 > /secrets/test-admin; printf "postgresql://erpx_test_admin:%s@127.0.0.1:5432/postgres" "$(cat /secrets/test-admin)" > /secrets/test-admin-url'
    if [ "$services_workspace_mode" = true ]; then
      # Only the tiny configuration image is rebuilt; source edits never
      # rebuild the toolchain or application images.
      services_compose build https
    fi
    services_compose up -d
    if [ "$services_workspace_mode" = true ]; then
      # Scanner definitions contain public signatures; grant search/read only to
      # its existing group, which is the native worker's sole supplemental group.
      attempt=0
      until services_compose exec -T -u 1000 scanner sh -ec 'test "$(stat -c %u:%g /var/lib/clamav)" = 1000:1000; chmod 0750 /var/lib/clamav'; do
        attempt=$((attempt+1)); [ "$attempt" -lt 30 ] || { echo SCANNER_DEFINITIONS_ACCESS_UNAVAILABLE >&2; exit 1; }; sleep 1
      done
    fi
    services_compose exec -T postgres sh -ec '
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
}
