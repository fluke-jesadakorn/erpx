# G3–G6 platform-completion runtime adapter

This slice owns only:

- `erpx/apps/worker/src/platform_completion_runtime.rs`
- this audit directory

It does not modify manifests or lockfiles, protobuf, migrations, worker
`lib.rs`/`main.rs` registration, server `main.rs`, the console shell, plans, or
ledgers. Coordinator-owned registration and server-composition evidence is
recorded below; this audit does not treat that source wiring as live acceptance.

## Delivered runtime boundary

`PlatformCompletionRuntimeAdapter` composes the existing
`PlatformControlStore` and `PlatformCompletionProjector`:

- command mutations are delegated to platform persistence, whose transactions
  use the control write pool and append the authoritative snapshot, outbox, and
  audit record atomically;
- sanitized platform reads remain delegated to persistence, whose read-model
  queries use the control read pool;
- projector writes are constructed only from the control read pool through
  `SqlxPlatformReadModelWriter`;
- outbox claims and projection checkpoints remain owned by the durable
  `OutboxWorker` on the control write side;
- rebuilds clear/replay through the existing `PlatformCompletionProjector`,
  with no in-memory projection fallback; and
- callers cannot supply database URLs, pools, credentials, replacement keys, or
  a locally invented identity to this adapter.

Every mutation wrapper validates the existing shared
`PlatformAuthorizationProof` before persistence. Read wrappers require the
shared platform-read permission and a valid proof. The wrapper preserves the
domain's MFA requirements for high-risk operations and its local,
platform-only, ticket-bound break-glass rule. Event handling fails closed when
the platform event has no actor binding, correlation/idempotency binding,
request hash, or credential/candidate-safe read model.

## Focused tests

The owned source contains eight focused unit tests for:

- exact shared read permission and platform scope;
- rejection of a holding-scoped proof when an opaque holding label cannot be
  converted to an exact typed authorization scope;
- MFA enforcement for high-risk platform mutations;
- local-only, ticket-bound break-glass authorization;
- actor/request-hash/read-model event binding;
- recursive credential/candidate projection-key rejection; and
- bounded worker-handler configuration.

The coordinator has now declared and re-exported this module in worker
`lib.rs`, so the focused tests execute as a Cargo target. These tests still
prove policy and bounded configuration only; no mock or schema-only result is
claimed as live SQLx or outbox acceptance.

## Coordinator integration evidence

The current coordinator-owned wiring is:

1. `erpx/apps/worker/src/lib.rs` declares and re-exports
   `PLATFORM_COMPLETION_RUNTIME_HANDLER_NAME`,
   `PlatformCompletionRuntimeAdapter`, and `PlatformCompletionRuntimeHandler`.
   Its default `WorkerRuntime::handler_registry()` currently registers both
   `platform_completion` and `platform_completion_runtime`. The registry test
   `registry_includes_support_expiry_handler` asserts the runtime handler name;
   this is registration evidence, not proof that both handlers may run at
   once. Production execution must select exactly one claimant for the
   platform outbox stream.
2. `erpx/apps/server/src/main.rs` constructs `PlatformCompletionApi` from
   `PlatformControlStore`, attaches `PlatformOperationsServiceServer`, and
   calls the authenticated `read_reconciliation_ui` path from the tenant
   detail RPC after platform scope authorization.
3. `erpx/apps/worker/src/main.rs` keeps Operations and Finance completion
   handlers as explicit `ERPX_WORKER_HANDLER` branches. The platform runtime
   is currently composed through the default registry rather than an explicit
   `ERPX_WORKER_HANDLER=platform_completion_runtime` branch.
4. Verify the existing
   `erpx/crates/migration/migrations_read/0010_platform_completion.sql` is
   applied to the actual control-read database and that the control write and
   control read URLs remain distinct under the existing pool configuration.
5. Exercise the selected registered worker against retained platform outbox rows:
   claim on control write, project on control read, checkpoint on control
   write, retry on unavailable read model, and rebuild by clearing/replaying
   retained events. Verify a failed projection never mutates authoritative
   write-side state.

## Missing live evidence

This slice does not claim the following evidence:

- live two-control-database SQLx transaction, outbox, checkpoint, projection,
  and rebuild evidence;
- observed physical proof that command writes never reach the read pool and
  projection reads never reach the write pool;
- worker retry/dead-letter/rollback behavior against a real unavailable read
  model;
- end-to-end live server authorization/API/UI evidence for G3–G6;
- production backup/PITR, replacement-read, connector-provider, or migration
  cutover evidence;
- generic LDAP, on-prem AD DS, or UCS credentialed certification; or
- production cutover, customer-directory activation, Folio write retirement,
  or command-owner transfer.

These are certification and coordinator-integration gaps, not reasons to add a
mock implementation to this slice.

## Verification performed

```text
rustfmt --edition 2024 --check \
  erpx/apps/worker/src/platform_completion_runtime.rs
```

Passed. An isolated Cargo harness that included this exact source file (while
leaving the repository's coordinator-owned registration files untouched) also
passed:

```text
cargo clippy --lib --tests -- -D warnings   # pass
cargo test --lib -- --nocapture             # 8 passed, 0 failed
```

Current repository-focused verification after coordinator integration:

```text
cargo test -p erpx-worker --lib platform_completion_runtime --offline -- --nocapture
# 8 passed, 0 failed
cargo test -p erpx-worker --lib registry_includes_support_expiry_handler --offline -- --nocapture
# 1 passed, 0 failed; asserts platform_completion_runtime is in the default registry
cargo test -p erpx-server --bin erpx-server platform_tenant_page_size_is_defaulted_and_bounded --offline -- --nocapture
# 1 passed, 0 failed
```

These are focused source/registration checks. The full G3–G6 gate additionally
requires the live evidence above and the workspace-level verification commands
from the project instructions.

## Rollback

Before any production rollback, disable or select exactly one of the two
currently registered platform handlers and rebuild the disposable platform
projection from retained outbox events. Do not delete append-only outbox or
audit rows. Removing this adapter file is safe before registration; after
registration, rollback is the normal worker feature-disable plus projector
rebuild, with authoritative control-write state unchanged. Live rollback
execution and receipt remain unproven.

## Acceptance status

| Requirement | Status |
| --- | --- |
| Reuses platform persistence | Implemented in owned runtime adapter |
| Reuses `PlatformCompletionProjector` | Implemented in owned runtime adapter |
| Routed write/read boundaries | Encoded by construction; live route proof pending |
| Fail-closed authorization | Implemented and unit-tested in owned source |
| Durable worker registration | Coordinator wiring present; default registry currently exposes two platform handler names and live single-claim execution is pending |
| Live SQLx/projector/rebuild evidence | Pending coordinator environment |
| Server platform composition | Coordinator wiring present; live authenticated API/UI evidence pending |
| G3–G6 runtime acceptance | Open until single-claim wiring and live evidence pass |
