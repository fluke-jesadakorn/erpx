# G10-MKT-055 Marketing content workflow persistence

This packet closes the holding-write workflow persistence slice for Marketing
content. It does not claim public HTTP/gRPC submission transport, preview
serving, DNS/HTTP verification adapters, expiry workers, read projections for
workflow facts, Dioxus/SSR, or the wider G10 gate.

## Integrated behavior

- Preview grants require a valid holding-scoped template reference whose
  revision is the authoritative published version.
- Preview issue and revoke commands persist only a bounded SHA-256 token hash,
  use holding-scoped idempotency, and append attributed outbox events.
- Published form submissions re-read the authoritative form snapshot under a
  scoped transaction, validate required/unknown/bounded fields, persist the
  submission values in holding-write state, and append an event whose payload
  omits submitted values.
- Domain verification stores only a SHA-256 challenge hash, activates a domain
  only after an exact proof-hash match, and clears the hash on success.
- Domain binding requires the verified active domain, exact holding scope, and
  the currently published template revision; replacement is atomic and
  idempotent.
- Workflow commands share the existing advisory lock, request-hash conflict,
  idempotency receipt, outbox, and forced-RLS transaction boundary.

## Evidence

- cargo fmt --all --check — PASS.
- cargo clippy -p erpx-infrastructure --all-targets -- -D warnings — PASS.
- Focused infrastructure test compilation — PASS.
- Disposable two-database lifecycle proof — PASS; one ignored live test
  covered preview issue/replay/revoke, published form submission, domain
  challenge/verification/binding, counts, forced-RLS inspection, read rebuild,
  and exact cleanup. Temporary databases were dropped and residue was checked.
- Independent AG-QA audit confirmed the pre-packet gaps and reviewed the
  resulting boundaries. The audit also identified the remaining transport,
  worker, projection, typed form-validation, and adapter work.

## Explicitly open

Public form/preview transport, raw-token generation and verification adapters,
preview expiry/revocation workers, workflow read projections, content audit
read models, semantic email/phone/number/consent validation, retention and
privacy policy for submitted values, MinIO live object binding, server
authorization/audit integration, Dioxus/SSR, provider secret management,
remaining ERP contexts, migration cutover, and Folio write retirement remain
open.

No Folio behavior or Folio write ownership changed.
