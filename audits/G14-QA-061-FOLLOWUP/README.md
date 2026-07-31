# G14-QA-061 follow-up — verified corrections

The independent audit in `audits/G14-QA-061/README.md` identified four safe
defects that were corrected without closing either gate:

- Marketing delivery `get` now reads a forced-RLS disposable read projection;
  tenant read migration 0026 and an explicit `project_read_model` operation
  were added.
- Provider callbacks must supply the provider message reference recorded at
  acceptance before delivery can become terminal. Signature verification,
  callback event idempotency, provider-account binding, and an HTTP/gRPC
  ingress are still open.
- Scheduler renew, complete, fail, and stale/reclaim transitions now keep
  job, claim, and rebuild updates in one transaction and require one claim
  history row to change.
- Sealed scheduler claim-key responses are replay receipts only; the worker
  does not execute replayed claims.
- Scheduler payloads and persisted error text now have bounded application and
  database checks, with a regression test for oversized payload rejection.

Independent verification:

- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-infrastructure --lib` — PASS: 90 passed, 4 ignored.
- `cargo clippy -p erpx-worker --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-worker --lib` — PASS: 35 passed, 1 ignored.
- `cargo test -p erpx-migration` — PASS: 6 passed.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain --lib` — PASS: 128 passed.

Remaining audit findings are intentionally open: authoritative scheduler
authorization and route resolution, production multi-tenant worker/projector
composition, replacement-read activation, callback authentication and
trusted time, database tamper constraints,
live channel adapters, and full G6/G10 requirements.
