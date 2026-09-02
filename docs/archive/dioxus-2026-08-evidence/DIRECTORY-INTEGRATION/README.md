# ERPX directory integration boundary

## Delivered in this wave

The write-side identity package now contains three complementary boundaries:

- `crates/identity/src/directory/auth.rs` and `tls.rs` enforce certificate-
  validated LDAPS/StartTLS, bounded connect/upgrade/bind timeouts, immutable
  provider object keys, high-risk MFA step-up, and local break-glass-only
  fallback for generic LDAP, AD DS, and UCS.
- `crates/identity/src/directory/sync.rs` and `feeds.rs` provide bounded
  full-import plans up to 1,000,000 users, ordered cursor/checkpoint semantics,
  RFC 4533, AD DirSync, UCS Provisioning Service, and generic LDAP feed seams,
  immutable identity links, append-only source events, directory-managed
  full-import role facts, and five-minute revocation signals. Finalization
  hydrates the identity-link and revocation read projections atomically with
  canonical state.
- `crates/identity/src/directory.rs` provides the coordinator-integrated typed
  connector store, import job persistence, role mapping facts, transactional
  identity projection, redacted directory outbox, and fail-closed revocation
  read boundary. `apps/server` exposes authenticated directory connector/import
  and role-mapping RPCs. `apps/worker` registers `directory_sync` but refuses to
  claim import work until a real external protocol adapter is configured.

The identity migration remains write-side only. It persists connector metadata,
import checkpoints/pages/members, immutable links, role facts, revocation SLA
rows, rebuildable projection rows, and redacted outbox payloads. Credentials,
bind packets, cookies, and certificate material are never persisted as secrets.

## Acceptance matrix

| Requirement | Evidence | Status |
|---|---|---|
| Generic LDAP | Provider/feed contracts, TLS policy, RFC 4533 seam, focused tests | Code boundary complete; live vendor certification queued |
| AD DS | Provider/feed contracts, DirSync seam, objectGUID identity key contract | Code boundary complete; live vendor certification queued |
| UCS | Provider/feed contracts, Provisioning Service seam, entryUUID identity key contract | Code boundary complete; live vendor certification queued |
| Platform + holding connector scopes | Typed scope validation, server permission boundary, SQLx scope key | Implemented |
| Full imports to 1M users | Bounded import plan, page/checkpoint/member persistence, rollback state | Implemented; live volume proof pending |
| Directory-managed RBAC | Worker passes connector mappings into SQLx full-import finalization; scoped group membership appends grant/revoke facts, including missing-snapshot revokes; identity-link/projection hydration is transactional; server effective-role authorization consults the identity write-side directory projection before local roles | Implemented locally; disposable PostgreSQL acceptance passed role-scope, full-import role/projection, allowed/disabled/stale shared-authorization tests on 2026-07-28; credentialed vendor and complete high-risk journey evidence remain queued |
| Native change feeds | RFC 4533, AD DirSync, UCS adapter interfaces and cursor rules | Adapter execution certification queued |
| Five-minute revocation | `DIRECTORY_REVOCATION_SLA`, stale-feed fail-closed projection, SLA table | Implemented |
| TLS-only communication | LDAPS/StartTLS endpoint and certificate policy | Implemented |
| High-risk MFA | Step-up policy and server permission boundary | Implemented at policy boundary; live MFA integration remains shared-auth evidence |
| Immutable linking | Stable provider keys, hash/fingerprint links, conflict checks | Implemented |
| Append-only revocation | Role facts and durable revocation SLA records | Implemented |
| Local fallback | Explicit, audited break-glass-only contract | Implemented |

## External certification blocker

No credentialed LDAP, on-prem AD DS, or UCS environment is available in this
workspace. Do not claim vendor acceptance. Queue the following certification
run for the environment owner:

1. Verify LDAPS and StartTLS certificate chain, hostname binding, trust-anchor
   rotation, timeout enforcement, and no-plaintext fallback for each vendor.
2. Run a one-million-user bounded import on disposable infrastructure and
   verify page restart, cursor continuity, duplicate suppression, reconciliation,
   and rollback.
3. Exercise RFC 4533 cookie continuity, AD DirSync invocation/cookie behavior,
   and UCS Provisioning Service reconnect/replay.
4. Disable/delete a directory user and measure role/access revocation under five
   minutes, including stale-feed fail-closed behavior and append-only evidence.

Until that run is complete, the directory worker remains opt-in and
fail-closed. Customer directory activation, production command-owner changes,
Folio write retirement, and final cutover remain explicitly out of scope.

## Rollback

Disable the connector and `directory_sync` worker, preserve connector/link,
role-fact, SLA, outbox, and audit history, and rebuild the read projection from
the last accepted source cursor. A failed import deletes only staged member
rows and moves the import to `rolled_back`; immutable identity links and
published events are never deleted or rewritten.
