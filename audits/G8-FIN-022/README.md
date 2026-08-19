# G8-FIN-022 — Finance bank-provider control-plane account bindings

## Task contract

- **Task ID:** G8-FIN-022
- **Owner:** AG-FIN
- **Objective:** Add a production-safe control-plane registry and exact
  resolver for Finance bank-provider account bindings and opaque credential
  references.
- **Dependencies:** `erpx_control.holding_registry`, `SqlxControlPlane`, and
  the provider-neutral Finance banking boundary.
- **Database scope:** control-plane write database only. No holding banking
  tables or holding credentials are changed.
- **Migration:** `0027_finance_bank_provider_accounts.sql`.

## Delivered behavior

The control plane now has `erpx_control.finance_bank_provider_accounts` with:

- globally unique opaque `provider_account_ref`;
- provider key;
- holding and holding binding, including authoritative `holding_id`;
- opaque `credential_key_ref` for a secret-manager lookup;
- `active`/`suspended` state and positive version;
- timestamps and an exact route lookup index;
- a restrictive foreign key to `holding_registry`.

`SqlxControlPlane::resolve_finance_bank_provider_account` validates every
reference before querying and requires exact account, provider, holding,
holding, and credential-reference equality. It returns no row for a suspended
account, inactive holding, or any mismatched route component. The returned DTO
contains only routing metadata and an opaque credential key reference; its
custom `Debug` implementation redacts that reference and it is not
serializable by default.

No provider SDK calls, credential-manager calls, provider payloads, holding
banking writes, bank matching, journal posting, worker scheduling,
gRPC/protobuf, UI, or Marketing code was added.

## Files changed

- `crates/migration/migrations/0027_finance_bank_provider_accounts.sql`
- `crates/migration/src/lib.rs`
- `crates/infrastructure/src/repositories.rs`
- `crates/infrastructure/tests/finance_banking_provider_accounts.rs`
- `audits/G8-FIN-022/README.md`
- `MASTER_PLAN.md`
- `EXECUTION_LEDGER.md`

## Verification

Focused checks:

```text
cargo fmt --all --check
cargo check -p erpx-infrastructure
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
cargo test -p erpx-migration control_plane_migration_is_embedded_and_has_the_expected_schema
cargo test -p erpx-infrastructure --test finance_banking_provider_accounts
ERPX_TEST_CONTROL_WRITE_DATABASE_URL=... \
ERPX_TEST_CONTROL_READ_DATABASE_URL=... \
cargo test -p erpx-infrastructure --test finance_banking_provider_accounts -- --ignored --nocapture
```

The non-disposable checks cover compilation, linting, embedded migration
assertions, and test compilation. The ignored PostgreSQL test applies clean
control write/read migrations and covers exact active resolution, wrong
provider/holding/holding/credential rejection, suspended-account rejection,
inactive-holding rejection, malformed-reference rejection, positive version,
foreign-key-backed registration, and redacted DTO diagnostics. Disposable
database URLs are never printed.

Recorded results:

- `cargo fmt --all --check` — PASS.
- `cargo check -p erpx-infrastructure` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure finance_provider_` — PASS: 2 focused
  repository unit tests; the disposable integration test remained ignored in
  this non-ignored run.
- `cargo test -p erpx-migration
  control_plane_migration_is_embedded_and_has_the_expected_schema` — PASS: 1
  migration assertion test.
- `cargo test -p erpx-infrastructure --test
  finance_banking_provider_accounts` — PASS: test compiled and 1 test ignored
  as intended.
- The ignored disposable command above — PASS: 1 exact-resolution integration
  test against two temporary PostgreSQL databases; both databases were
  removed by cleanup.

## Security and rollback

The table has no raw credential, token, provider payload, or secret-byte
column. A secret-manager key reference is matched exactly but redacted from
DTO debug output and errors contain no values. Holding activity and holding
identity are checked in the same authoritative lookup query. The provider
account reference is globally unique so a route cannot silently select an
alternate binding.

Rollback is additive: disable or ignore this resolver and retain migration
0027. No holding data, Folio state, environment file, or runtime service is
changed.

## Integration order and unfinished work

The main agent reviewed the migration, exact resolver, redaction tests, live
control-plane evidence, and exported `FinanceBankProviderAccountBinding` from
the infrastructure crate root. A future Finance provider boundary must use
this exact route, retrieve credential material through the secret manager,
invoke a provider SDK, and pass only validated facts into the existing
provider-neutral adapter. Provider SDKs, credential retrieval, bank-to-GL
matching, journal posting, worker workflows, transport, UI, Finance
reconciliation, and all remaining G8 requirements remain unfinished. This
packet does not claim G8 or ERPX completion.
