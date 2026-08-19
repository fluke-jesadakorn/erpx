# G10-MKT-056 Marketing workflow read projection and expiry foundation

This packet adds the separate holding-read workflow projection and the
registration-ready expiry execution path. It does not claim public transport,
automatic worker scheduling/registration in the executable, DNS/HTTP
verification adapters, preview serving, full content audit UI, or the wider
G10 gate.

## Integrated behavior

- Holding-read migration 0037 stores bounded preview metadata, form submission
  values, domain/binding state, workflow checkpoints, and event receipts under
  forced RLS.
- Preview token and domain verification hashes are excluded from the read
  schema and replacement rebuild.
- Replacement rebuild reads workflow facts from holding-write state, resolves
  authoritative outbox source metadata, clears only the scoped read rows, and
  repopulates preview, submission, domain, checkpoint, and receipt rows in one
  read transaction.
- Preview selection is bounded to one holding, due expiry, matching scope,
  and a maximum batch of 100.
- The expiry worker seam validates one holding/holding route, uses deterministic
  operation keys, continues after item failures, suppresses duplicates within
  a batch, and reports retryable versus permanent outcomes.
- The main-owned SQLx adapter derives deterministic command/correlation IDs,
  invokes the holding-write expiry command, and maps database failures to
  bounded retry behavior.

## Evidence

- cargo fmt --all --check — PASS.
- cargo clippy --workspace --all-targets -- -D warnings — PASS.
- cargo test --workspace --quiet — PASS: 173 domain tests; 158
  infrastructure tests passed with 4 ignored; 75 server tests; 108 worker
  tests passed with 1 ignored.
- Worker-focused strict Clippy/tests — PASS: 108 passed, 1 ignored.
- Dioxus web and desktop checks — PASS.
- Disposable two-database Marketing proof — PASS; preview expiry, form/domain
  workflow writes, separate workflow read rebuild, counts, RLS, and exact
  cleanup were verified. Temporary databases were dropped and residue was
  checked.
- Folio lint/typecheck — PASS; lint has four pre-existing warnings and zero
  errors.
- Folio /operations — HTTP 307; PostgreSQL readiness and the port-3004 dev
  server — PASS.
- git diff --check — PASS.

## Explicitly open

Worker executable scheduling/registration and durable job claim/retry
integration, public form submission and preview-serving transport, DNS/HTTP
verification adapters, semantic typed-field validation, privacy retention and
erasure, MinIO live object transport, server authorization/audit, Dioxus/SSR,
remaining ERP contexts, migration cutover, and Folio write retirement remain
open.

No Folio behavior or Folio write ownership changed.
