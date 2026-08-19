# G1 readiness enumeration — 2026-07-28

## Scope

The server readiness boundary previously accepted only one
`ERPX_READINESS_HOLDING_ID`, so `/readyz` could not establish readiness across
the platform's active holding registry. This packet preserves explicit holding
diagnostics and adds bounded platform-wide enumeration.

## Authoritative behavior

- Active registrations are loaded through `SqlxHoldingStoreRegistry::list_active_bounded`.
- The hard bound is 1,000 holdings and SQL fetches one sentinel row to reject
  an oversized active registry without unbounded memory use.
- Each active holding is probed through `HoldingDatabaseRouter` for live write/read
  routing, storage, projection freshness, and backup/PITR state.
- Component aggregation is fail-closed: `Unavailable` dominates,
  `NotProbed` blocks readiness, and only all `Ready` observations yield
  `Ready`; an empty registry is `NotProbed`.
- Registry or holding probe errors are logged without URLs, credentials,
  SQLx details, or holding registry references in the unauthenticated response.
- No production holding, directory, command owner, Folio write path, or cutover
  state was changed.

## Evidence

```text
cargo fmt --all --check
cargo test -p erpx-server --offline
```

Both passed. The focused server suite reported 87 passed and 1 ignored
database-dependent test. New tests cover aggregation precedence and the
infrastructure-to-server readiness mapping.

The post-packet current-tree verification also passed workspace formatting,
warnings-denied Clippy, the non-ignored workspace suite, protobuf package
tests, G13/G14 structural checks, all available Dioxus target checks, Folio
lint/typecheck, and the port-3004 health probe. The required ignored sweep
still exits at exactly 66 missing-resource/certification targets; those remain
queued rather than being converted into mocks or completion claims.

## Remaining certification

Credentialed Generic LDAP/RFC 4533, on-premises AD DS/DirSync, and UCS
Provisioning Service environments remain unavailable for certification, as do
the approved protobuf descriptor baseline, true PITR/WAL evidence, physical
mobile execution, and explicit production migration/cutover approval.
