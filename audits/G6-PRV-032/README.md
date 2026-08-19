# G6-PRV-032 — Two-person holding owner seeding

## Delivered

- `BeginHoldingProvisioning` now requires an owner principal reference and a
  distinct owner approver reference.
- The approver must be an active independent platform principal with the
  provisioning permission.
- Control migration `0057_holding_owner_seeding` stores only holding/holding,
  principal, approver, deterministic assignment, state, and recovery issuance
  metadata. It is not applied to the control read database.
- The owner seed is holding-bound and immutable on replay.
- Provisioning resources creates, in one control write transaction, a
  recovery-required identity, one Argon2id-hashed recovery code, and a finite
  90-day `holding_owner` role assignment with independent approval.
- Existing active principals can receive the assignment without a new
  recovery code; newly created/recovery-required principals receive one code
  only in the first successful response.
- Replaying the resource command is idempotent and never reissues or returns
  the recovery code.
- The recovery code is never logged, persisted in plaintext, or projected.

## Evidence

- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-server --all-targets -- -D warnings` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib provisioning::tests` — PASS; 13
  tests.
- `cargo test -p erpx-identity --lib` — PASS; 20 tests.
- Disposable two-database provisioning proof
  `owner_seed_registration_is_holding_bound_and_idempotent` — PASS; migration
  `0057` applied to the write database and absent from the read database.
- Disposable two-database server proof
  `holding_owner_seed_tests::owner_seed_transaction_creates_recovery_identity_and_finite_role`
  — PASS; identity recovery state, hashed recovery row, finite approved role,
  idempotent replay, and read isolation verified.
- Full integration wave — PASS: workspace format, clippy, tests, Dioxus
  web/desktop checks, Folio lint/typecheck, diff check, PostgreSQL/Folio/MinIO
  runtime checks.

## Remaining scope

This packet does not close the G6 gate. Physical credentialed MinIO proof,
true PostgreSQL PITR/WAL recovery, organization aggregate initialization in the
per-holding holding write schema, provider onboarding, device execution, and
the remaining G6/G12/G14 requirements remain open.
