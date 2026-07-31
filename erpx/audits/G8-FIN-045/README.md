# G8-FIN-045 — Finance candidate activation

## Scope

Disposable PostgreSQL evidence for the Finance subledger replacement-read
path. The test creates isolated control-write/control-read, tenant-write,
tenant-read, and candidate-read databases; seeds one authoritative Finance
source aggregate; executes the public routed candidate rebuild executor; and
verifies activation and projection on the candidate database.

## Independent evidence

- `cargo test -p erpx-worker --test finance_candidate_activation --no-run` — PASS.
- `ERPX_TEST_ADMIN_DATABASE_URL='postgresql:///postgres?host=%2Ftmp&port=5432' ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE='postgresql:///{database}?host=%2Ftmp&port=5432' cargo test -p erpx-worker --test finance_candidate_activation -- --ignored --nocapture` — PASS; 1 test.
- `cargo clippy -p erpx-worker --test finance_candidate_activation -- -D warnings` — PASS.
- `cargo fmt --all --check` — PASS after integration formatting.
- PostgreSQL cleanup query found no `erpx_fin045_*` databases after the run.

The proof covered candidate generation, source watermark fencing, physically
routed candidate read access, tenant registry activation, projected row and
checkpoint presence, and the candidate database identity. It did not prove
the complete Finance product gate, authenticated live gRPC transport, or
Folio cutover.

