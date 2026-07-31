# G6-PRV-022 — durable candidate rebuild dispatch

## Objective

Make an authenticated Operations replacement-read request durable from the
control-plane command through worker execution, while keeping the active read
database untouched.

## Delivered behavior

- Platform Operations validates the current holding-scoped Operations source
  high-watermark before accepting a rebuild request.
- One control-write transaction creates the generation-fenced replacement
  switch, typed scheduler job payload, rebuild record, and
  platform.projection_rebuild.start audit event.
- Candidate payloads require an opaque candidate database key, positive source
  version, and positive switch generation; partial or malformed payloads are
  rejected before routing.
- The worker provisions and probes the candidate through the server-only
  router, routes the current write database plus the exact fenced candidate,
  and never routes the active read database for candidate rebuild jobs.
- Candidate replay verifies the source watermark before and after replay.
  Source movement or projection failure fails the candidate generation before
  the scheduler can acknowledge the job.
- Ready-before-activation and activated-before-acknowledgement retries are
  idempotent and cannot clear an active read model.
- Finance Banking uses the same typed candidate payload and source-watermark
  checks in its routed scheduler executor.

## Files and interfaces

- crates/infrastructure/src/tenant_projection_scheduler.rs
- crates/infrastructure/src/provisioning.rs
- apps/server/src/main.rs
- apps/worker/src/tenant_projection_scheduler.rs
- apps/worker/src/lib.rs
- apps/worker/src/finance_banking_projection.rs
- crates/infrastructure/tests/tenant_projection_scheduler.rs
- crates/infrastructure/tests/provisioning_replacement.rs

No tenant credentials, URLs, payload secrets, or connection pools cross the
protobuf or worker error boundary.

## Verification

- cargo fmt --all --check — PASS.
- Targeted clippy for infrastructure, worker, and server — PASS.
- Worker, server, and infrastructure unit suites — PASS.
- Disposable control-plane scheduler integration — PASS; atomic switch/job/
  rebuild/audit persistence and platform-authorized binding were verified.
- Disposable physical provisioning/replacement-read integration — PASS;
  fenced candidate routing, RLS/identity probing, ready detection, and
  activation detection were verified.
- Disposable Operations projector integration — PASS; source event replay and
  generation-fenced candidate activation were verified.

## Remaining requirements

External backup/PITR execution, complete Platform Operations actions, broader
tenant contexts, full migration/cutover, native target evidence, hardening,
and Folio write retirement remain open. Scheduler retries after a failed
candidate generation require a new authenticated rebuild intent; no stale
generation is automatically reused.

## Amendment — source fence and terminal stale handling

The candidate worker now rejects an untyped rebuild before opening any tenant
route. Operations and Finance Banking source-write transactions acquire the
same holding-scoped transaction advisory lock used by the candidate worker.
After replay succeeds, the worker holds that lock while reading the source
watermark from the tenant write database, completing the candidate, and
activating the exact generation. Commit or rollback releases the lock
automatically. A source move is classified as `CandidateSourceMoved`, fails
the candidate generation, and is dead-lettered immediately instead of
consuming the ordinary retry budget. Transition failures remain retryable.

Additional verification:

- `cargo test -p erpx-infrastructure tenant_projection_scheduler::tests::stale_candidate_failure_is_terminal_without_consuming_retry_budget` — PASS.
- `cargo test -p erpx-worker --lib tenant_projection_scheduler::tests` — PASS; 7 tests.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- Disposable Operations routed projection — PASS; all disposable databases
  were dropped afterward.

The source fence covers the Operations and Finance Banking source tables
served by their typed write repositories. Other source contexts still require
their own producer and projection integration before this evidence can be
generalized to the full ERP.

## Amendment — activation recovery and long-running claims

The ready-candidate retry path now reacquires the holding transaction advisory
lock, rechecks the raw source watermark, and only then activates the exact
candidate generation. The authenticated Platform Operations activation route
uses the same locked path and is limited to the Operations projection; the
legacy switcher refuses direct replacement activation instead of bypassing the
fence. Initial Operations and Finance Banking source mismatches are classified
as `CandidateSourceMoved` and terminalized without consuming retry budget.

The scheduler renews every active projection claim at half of its configured
lease interval while the injected executor is still running. Renewal failure
returns a redacted worker error and does not acknowledge or fail a claim whose
ownership may have been lost.

Additional verification:

- `cargo test -p erpx-worker --lib tenant_projection_scheduler::tests` — PASS;
  8 tests, including long-running claim renewal.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --workspace` — PASS in the integration wave.
- Folio `bun run lint` — PASS with four existing n8n warnings; zero errors.
- Folio `bunx tsc --noEmit` — PASS.
- Dioxus console web and desktop checks — PASS.

The remaining medium risks are unchanged: source fencing is implemented only
for the typed Operations and Finance Banking producer paths; candidate failure
state and scheduler terminal acknowledgement are separate control transactions
and need crash-injection evidence; source high-watermarks remain bounded raw
event counts rather than durable monotonic source versions; and backup/PITR
execution is still an adapter seam. The legacy metadata-only backup activation
entry point now fails closed because it cannot hold a tenant source fence;
route-aware restore activation remains an open requirement.

## Amendment — route-aware restore-test activation

`TenantProvisioner::execute_restore_test_with_routed_read_replacement` now
composes the injected restore executor with a `TenantDatabaseRouter`. It
requires the provisioner and router to share the same control plane, validates
the tenant holding binding and opaque candidate key, acquires the routed write
database's holding-scoped transaction advisory lock, and compares the exact
source watermark before and after candidate route verification.

The candidate is routed only through the exact tenant/candidate/generation/
source fence. After the replacement route probe succeeds, completion and
promotion occur only through `activate_read_database_if_generation` while the
source lock remains held. Source mismatch, route mismatch, stale generation,
and activation failures leave the previous active read route selected and mark
the restore run failed with bounded error text. The receipt exposes only the
opaque candidate key, generation, source version, and safe backup-run state;
URLs, credentials, and pools are not exposed.

Focused evidence:

- `cargo fmt` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS; 96 passed, 4 ignored.
- `cargo test -p erpx-infrastructure --test provisioning --no-run` — PASS.
- `cargo test -p erpx-infrastructure --test provisioning_replacement --no-run` — PASS.
- Disposable `provisioning_replacement` integration — PASS; stale-source
  failure and successful route-aware activation both passed, with disposable
  databases cleaned by the test.

The adapter still depends on an injected external backup/PITR executor for
actual restore work. Source fencing currently supports the typed Operations
and Finance Banking producer paths; other source contexts require their own
bounded watermark implementation before using this API.

## Rollback

Remove only the additive atomic dispatch method, candidate scheduler branch,
fenced route helpers, and this audit. Existing active tenant read routing is
unchanged by rollback.
