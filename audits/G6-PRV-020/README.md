# G6-PRV-020 — candidate read provisioning, projector binding, and fenced activation

## Objective

Close the physical replacement-read seam for one concrete holding projection
without exposing database credentials or allowing a stale worker to switch a
newer candidate.

## Delivered behavior

- `HoldingDatabaseProvisioner::provision_replacement_read_database` creates a
  candidate database through the server-only administrative URL, applies the
  holding-read migration set, and requires an active holding with a matching
  `rebuilding` switch state.
- Candidate keys are serialized through a per-key PostgreSQL advisory lock and
  rejected when another holding owns the key in its registry or active switch.
- `HoldingDatabaseRouter::route_replacement_read` binds the candidate to the
  holding/holding switch row, verifies the registry current-read binding, and
  probes the physical database identity, required read relations, forced RLS,
  policies, and holding isolation before returning a pool.
- `OperationsProjectionRunner` implements `CandidateProjectionRebuilder` and
  clears/replays the Operations read model against the explicitly routed
  candidate pool while reading immutable source events from the write pool.
- `CandidateReadDatabasePreparer` is an injected worker seam; the production
  `HoldingDatabaseProvisioner` implementation validates holding/holding binding,
  creates the candidate, and applies the complete holding-read migration set
  before the projector receives the route.
- Replacement completion, failure, and activation support a monotonic switch
  generation. The worker switcher uses that generation for compare-and-swap
  transitions; stale candidate workers cannot activate a later rebuild.
- Platform Operations `StartProjectionRebuild` returns the generation after
  recording an authenticated audited rebuild intent. `ActivateProjectionRebuild`
  requires the same platform permission and actor binding and activates only
  the exact ready candidate generation.

## Files and contracts

- `crates/infrastructure/src/provisioning.rs`
- `crates/infrastructure/tests/provisioning_replacement.rs`
- `apps/worker/src/lib.rs`
- `apps/worker/tests/operations_projection.rs`
- `apps/server/src/main.rs`
- `crates/proto/proto/erpx/platform/v1/platform.proto`

No holding URL, credential, storage namespace, or candidate pool is serialized
by the protobuf contract.

## Verification evidence

- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-worker` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 49 active tests, one documented
  disposable control-database test ignored.
- `cargo test -p erpx-server --bin erpx-server` — PASS: 63 tests.
- `cargo test -p erpx-infrastructure --test provisioning_replacement
  -- --ignored --nocapture` — PASS against disposable PostgreSQL databases.
  The test creates and migrates a physical candidate, probes it, exercises a
  generation-bound completion/activation, verifies failed restore work does
  not switch the active read database, and removes all disposable databases.
- `cargo test -p erpx-worker --test operations_projection -- --ignored
  --nocapture` — PASS against disposable control/write/read/candidate
  PostgreSQL databases. The test replays one Operations event into the
  candidate, completes and activates the exact generation, verifies the
  active registry read key, and removes all disposable databases.

The full workspace gate remains mandatory after this packet. This packet does
not claim source high-watermark/catch-up verification, durable worker dispatch
from the Platform Operations intent, external backup/restore execution, or
completion of the broader G6/G12 platform operations requirements.

## Rollback

Remove the additive candidate provisioning method, Operations candidate
rebuilder implementation, generation-bound transition methods, activation RPC,
and focused evidence. Preserve the existing active-read registry and the
legacy non-candidate backup/restore seam until a reviewed replacement is
available. No active holding read key is changed by rollback itself.
