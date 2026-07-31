# G10-MKT-053 Marketing content and digital experience

This packet integrates the bounded domain contract and the first authoritative
SQLx persistence slice. It is evidence, not completion of the Marketing gate.

## Integrated behavior

- Tenant-scoped versioned content assets and template revisions.
- Required Thai and English content variants.
- Bounded SEO/social metadata and opaque object references.
- Draft/review/approval/schedule/publication/rollback domain rules.
- Expiring and monotonic revocable preview grants.
- Bounded form, submission, and custom-domain contracts in the domain layer,
  with tenant-write form/domain aggregate snapshots.
- Tenant-write asset/template snapshots with immutable revision rows.
- Optimistic aggregate versions and command idempotency receipts.
- Complete shared event envelopes and atomic tenant outbox writes.
- Forced-RLS write tables and a physically separate rebuildable read model.
- Read projection rows, checkpoints, and event receipts are rebuilt from write
  state without touching tenant credentials or object bytes.

## Independent evidence

- `cargo fmt --all --check` — PASS.
- `cargo clippy -p erpx-infrastructure --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain marketing_content` — PASS; 5 tests.
- `cargo test -p erpx-infrastructure --test marketing_content_persistence`
  — PASS; migration contract plus one ignored live proof.
- Disposable two-database live proof — PASS. It covered create, exact replay,
  optimistic version conflict, review/approval/publication, write outbox and
  revision counts, forced RLS, replacement-read rebuild/checkpoints, and
  cross-holding isolation. Exact databases were dropped afterward.
- `git diff --check` — PASS at integration review.

## Explicitly open follow-up work

The packet does not claim completion of domain binding/verification and form
submission commands, Marketing MinIO authorization and signed transport, DNS/domain verification, preview and
publication workers, Marketing gRPC/HTTP transport, SSR landing pages, Dioxus
editor/preview/domain UI, campaign/journey content effects, or the broader G10
gate. Those remain separate dependency-ready packets.

No Folio behavior or Folio write ownership changed.
