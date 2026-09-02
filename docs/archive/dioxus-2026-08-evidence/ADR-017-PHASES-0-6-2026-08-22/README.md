# ADR-017 phases 0-6 local completion evidence — 2026-08-22

## Verdict

ADR-017 phases 0-6 are code-complete in the current ERPX tree and passed the
repository's complete serialized local verification gate. The delivered
surface covers all 93 current record families. Future catalog records remain
fail-closed until they publish the complete owner, access, collaboration,
editable-field, and transport registration.

This is retained local implementation evidence. It is not evidence of a
production rollout, customer activation, production-data migration, external
provider or directory certification, malware-scanner readiness, or completion
of the staged canary.

## Delivered boundary

- Phase 0: authenticated actor kind and session-bound `HumanDecisionProof` are
  enforced at every registered decision-shaped ingress; owner domains validate
  the proof independently. The ingress inventory has no migration placeholder,
  and prior protobuf descriptor compatibility is a mandatory hashed-baseline
  gate.
- Phase 1: authoritative Organization lineage, exact Department/Team subtree
  scopes, safe reparenting, source-fenced projections, authorization-before-
  payload SQL snapshots, and explicit per-family access policies are in place.
  Unsupported exact-subtree and sharing forms deny access.
- Phase 2: the shared record host mounts owner content, typed references, and
  authorized activity for every current canonical route. Holding-local and
  audited Platform paths support relation add/revoke, typed thread V2,
  comments, reactions, following, and attachment lifecycle with fresh download
  authorization.
- Phase 3: structured `@` tokens, authorized autocomplete, confirmed Agent or
  pool assignment, pinned invocation provenance, and human review are wired
  through collaboration transport and Console gateways.
- Phase 4: owner-typed editable-field policies, durable suggestions, diff and
  evidence presentation, stale/version/source-digest guards, and human apply
  through owner commands are implemented. Unsupported fields do not accept an
  opaque fallback.
- Phase 5: Procurement owns requisition, purchase order, and goods receipt;
  Finance owns vendor invoice, payable, and payment. Immutable version-pinned
  evidence, three-way matching, outbox/projectors, fail-closed fresh reads,
  owner command capabilities, and same-origin Console owner details provide the
  durable PR-to-payment slice. Customer invoice remains a separate AR record.
- Phase 6: exact-set completion and architecture gates cover 93 current record
  publishers, policies, owner transports, canonical routes, shared surfaces,
  typed-token eligibility, Agent eligibility, editable fields, and human-only
  decision classification.

## Integration checkpoints

The implementation was committed as a stack. Principal phase checkpoints are:

- `7fef20fd` — universal record collaboration surfaces.
- `f5c91333`, `b6417848`, `7570fa47` — typed tokens, authorized Agent
  collaboration, and confirmed assignment.
- `9a73cf23`, `471e5309`, `a5d0ea38`, `8fe0cf09`, `42d21fbf` — typed field
  suggestion contracts, persistence, transport, activity UI, and human review.
- `896ab508`, `8b67e144`, `2e44a2ff`, `92d3fdcb`, `0e5c8213`, `34b589eb` —
  audited Platform collaboration, activity, attachments, and typed mentions.
- `deefd175` through `25d7b3ac` — durable Procurement-to-Finance workflow,
  projections, payables, orchestration, owner capabilities, and Console owner
  delivery.
- `2631d7af` — exact current-record completion enforcement.
- `56ce5dc2` — mandatory prior protobuf compatibility gate.
- `9b000998` through `006fcc5e` — owner proof migration and closed
  decision-ingress inventory.
- `c9096620`, `561e2875`, `870ba8d3`, `691f4497` — final runtime, migration,
  lint, and SQLx acceptance corrections discovered by the complete gate.

## Verification evidence

The following gates passed from the repository root on 2026-08-22:

- `cargo xtask check-affected --dry-run`
- `cargo xtask check-affected`
- `cargo xtask test-affected --dry-run`
- `cargo xtask test-affected`
- focused owner-domain, SQLx adapter, protobuf compatibility, Console owner
  delivery, completion-registry, and decision-ingress inventory suites
- `cargo xtask verify`

The final `cargo xtask verify` passed:

- formatting and warnings-denied workspace Clippy;
- the full workspace test and doc-test suite with default Web features;
- Console Web and Server checks;
- the Dioxus split-release Web build and WASM-size boundary;
- DDD/Clean/Hexagonal/CQRS and UI architecture checks;
- disposable PostgreSQL SQLx acceptance, including Organization hierarchy,
  Identity human-decision/session, Collaboration authorization, Procurement,
  Finance, CQRS, projection, and migration paths; and
- runtime readiness for 99 required worker processes and heartbeats.

After runtime reconciliation, `cargo xtask status` was healthy and the live
`/auth/sign-in` endpoint returned HTTP 200 with `text/html`.

Credentialed MinIO/PITR and LDAP/AD/UCS certification are intentionally
excluded by the local verifier and remain external gates. Production
attachments must remain disabled until the malware scanner passes readiness;
plain collaboration does not depend on that activation.

## Rollout and rollback boundary

Rollout remains additive and reader-first: deploy protobuf readers and
forward-only migrations, activate publishers/workers, rebuild and atomically
switch the Item Directory generation, canary Identity/Legal/Procurement/
Finance, then enable context batches. Attachment activation is a separate
scanner-readiness switch.

Rollback disables producers and UI capabilities, restores the prior active
projection generation, and replays replaceable read models from authoritative
outbox history. It must not delete owner aggregates, collaboration facts,
receipts, attachments, or migration history. No production state was changed
while producing this evidence.
