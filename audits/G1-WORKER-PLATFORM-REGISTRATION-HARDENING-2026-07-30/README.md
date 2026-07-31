# G1 worker platform-registration hardening — 2026-07-30

## Status

Integrated local hardening packet. This packet does not close G1 or any
production gate.

## Authoritative behavior

The historical `platform_completion` worker name remains available for
deployment compatibility, but it now delegates to the same
`PlatformCompletionRuntimeAdapter` used by `platform_completion_runtime`.
Both names therefore share the same control-write claim boundary, control-read
projection writer, worker identity check, bounded batch/lease configuration,
platform event actor/request-hash/read-model validation, and retry behavior.

There is no longer a legacy platform handler path that can project a platform
outbox event without the runtime safety gate. The two names are aliases for
compatibility and must still be selected one at a time by the deployment
handler setting; this packet does not start a second worker or change command
ownership.

## Scope and applicability

This is a coordinator-owned worker registration/composition correction. No
domain rule, SQLx schema, protobuf, API, console surface, customer directory,
or Folio write path is changed. The existing authoritative SQLx persistence,
outbox, read model, projector, authorization proof, and rollback/rebuild
contracts remain the sources of truth; this packet makes both executable
registration names use them uniformly.

## Evidence

- `cargo fmt --all --check` — PASS after formatting.
- `cargo test -p erpx-worker --offline --lib platform_completion` — 10 PASS.
- `cargo test -p erpx-worker --offline --lib registry_includes_support_expiry_handler` — 1 PASS.
- `cargo clippy -p erpx-worker --all-targets --offline -- -D warnings` — PASS.
- `git diff --check` — PASS.

The focused tests are local code evidence. No production worker was started,
no customer directory was activated, and no external provider or database was
mutated by this packet.

## Rollback

Reverting the registration delegation restores the former compatibility alias
but reopens the weaker-path risk. If rollback is required after deployment,
stop the selected platform worker, keep authoritative outbox and audit rows,
and rebuild the platform read model from retained events before resuming a
single selected handler name. No source rows are deleted.

## Remaining acceptance boundary

Live control write/read worker execution, restart/lease evidence, approved
protobuf provenance, external directory/provider certification, physical
PITR/device/scale evidence, Folio migration/cutover, and explicit production
approval remain queued. G0–G14 are not claimed complete.
