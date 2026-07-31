# G12 UI/QA — authenticated Platform Operations adapter coverage

Date: 2026-07-28  
Status: bounded UI/QA slice complete; shell integration and live environment
evidence remain outside this packet.

## Audit finding

The console already has coordinator-owned Platform Operations route adapters,
but the reusable G12 product model represented Platform Operations with one
aggregate persistence/authorization flag. That allowed an incomplete area set
to look covered without naming the adapter responsible for each authenticated
read.

## Delivered slice

- Added an allow-listed `PlatformOperationsAdapter` identity for the six safe
  Platform Operations areas: tenants, projections, backups, integrations,
  audit, and settings.
- Added `PlatformOperationsAreaEvidence` so each displayed area carries its
  matching adapter, authoritative persistence, reconciliation, and server
  authorization facts.
- Added `from_authoritative_area_reads`, which rejects missing, reordered, or
  mismatched area attestations and rejects mixed reconciliation evidence.
- Added `PlatformOperationsEvidenceSurface`, a read-only Dioxus surface that
  renders only complete authenticated adapter evidence and fails closed to the
  blocked state for metadata-only or incomplete models. It exposes no
  commands, credentials, endpoints, or provider payloads.
- Kept all shell, shared routing, protobuf, manifests, auth, server, worker,
  migration, and Folio files unchanged.

## Owned files

- `apps/console/src/product_completion.rs`
- `apps/console/src/qa_completion.rs`
- `audits/G12-UI-PLATFORM-ADAPTER-COVERAGE-2026-07-28/README.md`

## Focused verification

Run from `erpx/`:

```text
cargo fmt -p erpx-console -- --check
cargo clippy -p erpx-console --all-targets --offline -- -D warnings
cargo test -p erpx-console --bin erpx-console --offline --all-targets --no-fail-fast
cargo test -p erpx-console --bin erpx-console qa_completion::tests::platform_operations --offline -- --test-threads=1
cargo test -p erpx-console --bin erpx-console product_completion::tests --offline -- --test-threads=1
```

Results:

- Formatting: PASS.
- Console Clippy with warnings denied: PASS.
- Full console binary suite: **108 passed, 0 failed**.
- Platform Operations focused QA: **3 passed, 0 failed**.
- Product-completion contract suite: **12 passed, 0 failed**.

## Evidence boundary

These checks prove the UI contract and its fail-closed adapter coverage
boundary. They do not claim browser execution, a live gRPC session, external
directory certification, mobile device execution, or production Platform
Operations readiness. The new reusable surface was intentionally not mounted
through the coordinator-owned shell in this packet; mounting it requires the
existing live adapters and is outside this file-ownership slice.

## Rollback

Revert the three owned paths above. No database, runtime state, shared route,
server/worker registration, authorization policy, protobuf contract, or Folio
behavior depends on this additive UI/QA slice.
