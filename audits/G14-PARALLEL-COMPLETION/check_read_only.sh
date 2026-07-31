#!/usr/bin/env bash

set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
AUDIT_DIR="$ROOT_DIR/erpx/audits/G14-PARALLEL-COMPLETION"
failures=0

pass() {
  printf 'PASS  %s\n' "$1"
}

warn() {
  printf 'BLOCK %s\n' "$1"
}

fail() {
  printf 'FAIL  %s\n' "$1"
  failures=$((failures + 1))
}

require_file() {
  local path="$1"
  if [[ -f "$ROOT_DIR/$path" ]]; then
    pass "file exists: $path"
  else
    fail "missing required file: $path"
  fi
}

require_marker() {
  local path="$1"
  local marker="$2"
  local label="$3"
  if [[ -f "$ROOT_DIR/$path" ]] && rg -Fq "$marker" "$ROOT_DIR/$path"; then
    pass "$label"
  else
    fail "$label (marker not found)"
  fi
}

cd "$ROOT_DIR" || exit 1

printf 'G14 read-only structural probe\n'
printf 'root: %s\n' "$ROOT_DIR"
printf 'branch: %s\n' "$(git branch --show-current 2>/dev/null || printf 'unknown')"

if [[ "$(git branch --show-current 2>/dev/null || true)" == "codex/erpx-parallel-completion" ]]; then
  pass 'expected ERPX completion branch is checked out'
else
  warn 'expected branch codex/erpx-parallel-completion is not checked out'
fi

expected_files=(
  'erpx/MASTER_PLAN.md'
  'erpx/EXECUTION_LEDGER.md'
  'erpx/audits/DIRECTORY-INTEGRATION/README.md'
  'erpx/audits/DIRECTORY-AUTH/README.md'
  'erpx/audits/DIRECTORY-SYNC/README.md'
  'erpx/audits/G0-001_CURRENT_STATE_AUDIT.md'
  'erpx/audits/G13-MIG-001/README.md'
  'erpx/audits/G13-MIG-002/README.md'
  'erpx/audits/G14-QA-061/README.md'
  'erpx/audits/G14-QA-061-FOLLOWUP/README.md'
  'erpx/audits/G14-QA-062/README.md'
  'erpx/audits/G14-QA-062-FOLLOWUP/README.md'
  'erpx/crates/identity/src/directory/feeds.rs'
  'erpx/crates/identity/src/directory/rbac.rs'
  'erpx/crates/identity/src/directory/tls.rs'
  'erpx/apps/server/src/main.rs'
  'erpx/apps/worker/src/main.rs'
  'erpx/crates/proto/proto/erpx/platform/v1/platform.proto'
  'erpx/apps/console/Cargo.toml'
  '.github/workflows/erpx-targets.yml'
)

for path in "${expected_files[@]}"; do
  require_file "$path"
done

require_marker 'erpx/crates/identity/src/directory/feeds.rs' 'FeedKind::ActiveDirectoryDirSync' 'AD DirSync feed seam present'
require_marker 'erpx/crates/identity/src/directory/feeds.rs' 'FeedKind::Rfc4533' 'RFC 4533 feed seam present'
require_marker 'erpx/crates/identity/src/directory/feeds.rs' 'FeedKind::UcsProvisioningService' 'UCS Provisioning Service feed seam present'
require_marker 'erpx/crates/identity/src/directory/tls.rs' 'DirectoryTransportMode::Ldaps' 'LDAPS transport policy present'
require_marker 'erpx/crates/identity/src/directory/tls.rs' 'DirectoryTransportMode::StartTls' 'StartTLS transport policy present'
require_marker 'erpx/crates/identity/src/directory/rbac.rs' 'DIRECTORY_RBAC_MFA_WINDOW' 'directory RBAC MFA window present'
require_marker 'erpx/crates/identity/src/lib.rs' 'MAX_DIRECTORY_IMPORT_USERS' 'directory import bound exported'
require_marker 'erpx/crates/identity/src/lib.rs' 'DIRECTORY_REVOCATION_SLA' 'directory revocation SLA exported'
require_marker 'erpx/apps/worker/src/main.rs' 'DIRECTORY_SYNC_HANDLER_NAME' 'directory worker registration marker present'
require_marker 'erpx/apps/server/src/main.rs' 'DirectoryServiceServer' 'directory server registration marker present'
require_marker 'erpx/crates/proto/proto/erpx/platform/v1/platform.proto' 'service DirectoryService' 'directory protobuf service present'

allowed_count=0
while IFS= read -r -d '' path; do
  relative="${path#"$ROOT_DIR/"}"
  case "$relative" in
    erpx/audits/G14-PARALLEL-COMPLETION/README.md|erpx/audits/G14-PARALLEL-COMPLETION/check_read_only.sh)
      allowed_count=$((allowed_count + 1))
      ;;
    *)
      fail "unexpected file in this audit package: $relative"
      ;;
  esac
done < <(find "$AUDIT_DIR" -type f -print0)

if [[ "$allowed_count" -eq 2 ]]; then
  pass 'audit package contains only its two owned files'
else
  fail "audit package file count is $allowed_count, expected 2"
fi

for command_name in git rg cargo bun bunx curl; do
  if command -v "$command_name" >/dev/null 2>&1; then
    pass "command available: $command_name"
  else
    fail "command unavailable: $command_name"
  fi
done

folio_code='000'
if command -v curl >/dev/null 2>&1; then
  if ! folio_code="$(curl --silent --show-error --output /dev/null --write-out '%{http_code}' --max-time 5 http://127.0.0.1:3004/operations 2>/dev/null)"; then
    folio_code='000'
  fi
fi
if [[ "$folio_code" == 3* ]]; then
  pass "Folio /operations responds with HTTP $folio_code on port 3004"
else
  warn "Folio /operations probe returned HTTP $folio_code; keep the dev server running on port 3004"
fi

printf 'scope: git status for owned package only\n'
git status --short -- erpx/audits/G14-PARALLEL-COMPLETION

if [[ "$failures" -eq 0 ]]; then
  printf 'RESULT PASS: structural checks only; no gate completion is claimed.\n'
else
  printf 'RESULT FAIL: %s structural check(s) failed.\n' "$failures"
  exit 1
fi
