#!/bin/sh
# Run only inside a disposable source clone in the owned Linux verification container.
set -eu
umask 077
[ "$(uname -s)" = Linux ] && [ -f /.dockerenv ] || { echo ISOLATED_LINUX_CONTAINER_REQUIRED >&2; exit 2; }
[ "${GITHUB_ACTIONS:-}" != true ] || { echo LOCAL_RUN_MUST_NOT_IMPERSONATE_GITHUB >&2; exit 2; }
[ "$#" -eq 2 ] || { echo 'Usage: local-verification.sh <base-sha> <head-sha>' >&2; exit 2; }
base=$(git rev-parse --verify "$1^{commit}")
head=$(git rev-parse --verify "$2^{commit}")
[ "$(git rev-parse HEAD)" = "$head" ] && [ -z "$(git status --porcelain)" ] || { echo CLEAN_EXACT_CANDIDATE_REQUIRED >&2; exit 2; }
export CI=true ERPX_LOCAL_VERIFICATION=owned-linux-container
export ERPX_NODE_BIN=/usr/local/bin/node ERPX_BUN_BIN=/usr/local/bin/bun
export ERPX_CI_CARGO_PROFILE=compact-v1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
: "${ERPX_TEST_ADMIN_URL:?Owned disposable administrator required}"
: "${ERPX_CI_POSTGRES_MONITOR_URL:?Owned isolated PostgreSQL monitor required}"
: "${ERPX_CLAMD_SOCKET:?Real scanner socket required}"
: "${ERPX_CLAMAV_DATABASE_DIR:?Real scanner definitions required}"
: "${ERPX_CLAMAV_VERSION:?Observed scanner engine version required}"
: "${ERPX_NATIVE_VIPS_PREFIX:?Private native prefix required}"
mkdir -p .scratch/dev/local-verification
directory=$(mktemp -d .scratch/dev/local-verification/attempt.XXXXXXXX)
printf '%s\n' incomplete > "$directory/result"
printf '%s\n' "$base" > "$directory/base-sha"
printf '%s\n' "$head" > "$directory/head-sha"
uname -sm > "$directory/platform"
run() {
  stage=$1; shift
  echo "LOCAL_VERIFICATION_STAGE:$stage"
  if "$@" > "$directory/$stage.log" 2>&1; then
    printf '%s\n' passed > "$directory/$stage.result"
  else
    code=$?
    printf 'failed:%s\n' "$code" > "$directory/$stage.result"
    echo "LOCAL_VERIFICATION_FAILED:$stage (private log retained)" >&2
    exit "$code"
  fi
}
run dependencies bun install --frozen-lockfile
run postgres-startup bun scripts/testing/ci-postgres-resource.ts --probe-only
run attachment-fixture bun scripts/testing/fixtures/generate-attachment-heic.ts --check
run attachments bun --conditions=react-server scripts/testing/ci-attachment-prerequisites.ts
run browser bunx playwright install chromium
run native-prefix bun scripts/testing/provision-native-worker.ts --prefix="$ERPX_NATIVE_VIPS_PREFIX"
export PKG_CONFIG_PATH="$ERPX_NATIVE_VIPS_PREFIX/lib/pkgconfig"
export ERPX_NATIVE_VIPS_LIBDIR="$ERPX_NATIVE_VIPS_PREFIX/lib"
CARGO_ENCODED_RUSTFLAGS=$(bun -e 'import {nativeWorkerLinkFlags} from "./scripts/testing/native-worker-link-flags"; process.stdout.write(nativeWorkerLinkFlags("linux",process.env.ERPX_NATIVE_VIPS_LIBDIR!))')
export CARGO_ENCODED_RUSTFLAGS
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(pwd)/target}"
export ERPX_FILES_PREVIEW_BIN="$CARGO_TARGET_DIR/debug/erpx-render-preview"
run native-check bun scripts/testing/native-worker-toolchain.ts check
run native-clippy bun scripts/testing/native-worker-toolchain.ts clippy
run native-build bun scripts/testing/native-worker-toolchain.ts build
run native-link bun scripts/testing/native-worker-toolchain.ts inspect-linked
run affected bun scripts/testing/ci-postgres-resource.ts -- bun run test:ci --base="$base" --head="$head"
[ "$(git rev-parse HEAD)" = "$head" ] && [ -z "$(git status --porcelain)" ] || { echo CANDIDATE_CHANGED_DURING_VERIFICATION >&2; exit 2; }
printf '%s\n' 'All declared local stages passed; this is local evidence, not a hosted status or production acceptance.' > "$directory/result"
echo LOCAL_VERIFICATION_PASSED
