# G10-MKT-077 — Marketing delivery projection runtime and RFC 4533 import continuation

Date: 2026-07-28

## Delivered behavior

- The tenant-read schema now has a durable Marketing delivery projection
  cursor ordered by authoritative outbox timestamp and event ID.
- The multi-tenant `marketing_delivery` worker projects bounded delivery
  pages before provider claims. Source envelopes, aggregate identity, event
  type, and aggregate version are checked; read rows remain version fenced.
- A worker crash between read-row commit and cursor commit replays the source
  event safely. The disposable delivery read model can be cleared without
  touching the write aggregate or outbox, and the next pass rebuilds it.
- `reconcile_projection` reports source event/delivery counts, missing or stale
  read rows, and orphan rows without repairing authoritative state.
- RFC 4533 now continues all cookie-bearing full-import pages under the same
  bounded one-million-user guard as LDAP polling, AD DirSync, and UCS
  Provisioning Service imports.
- The authenticated Marketing content service now exposes a typed delivery
  read RPC bound to tenant, holding, and caller scope. The console renders only
  delivery state, attempts, retry/expiry timestamps, immutable version, and the
  read-only contract; recipient payloads and provider material remain absent.

## Evidence

- `cargo fmt --all --check` — pass.
- Infrastructure and worker focused checks, including warnings-denied Clippy,
  — pass.
- Directory worker continuation tests cover all four native feed kinds and
  cookie/max-user stop conditions.
- Console delivery-boundary tests pass, including scope binding, UUID/state
  validation, and payload-free decoding; the Marketing Channels route now
  renders the same typed read model.
- Ignored SQLx acceptance target
  `delivery_projection_worker_path_advances_a_durable_cursor` compiles and
  fails closed when disposable tenant write/read URLs are absent. With the
  required resources it seeds an authoritative enqueue event, runs the same
  projection path used by the worker, asserts a durable checkpoint and read
  row, runs reconciliation, and confirms an idempotent second pass.

The existing authenticated provider callback API, provider-account binding,
inbox idempotency, transition outbox events, and multi-tenant worker
composition remain the authoritative G10 delivery boundaries. This packet did
not activate an account or contact an external provider.

## Rollback

Revert the additive read migration, projection cursor/reconciliation code,
worker call, RFC 4533 continuation helper, focused test, and this audit. That
removes only disposable read state and runtime consumption; it does not delete
or rewrite tenant-write deliveries, callback inbox records, immutable
identity links, or provider configuration.

## Remaining certification

Disposable tenant write/read PostgreSQL URLs, real provider callback/provider
delivery certification, approved protobuf descriptor comparison, and final
G10/G14 re-audit remain queued. No production command owner switch, directory
activation, Folio write retirement, or cutover occurred.

## Coordinator refresh — 2026-07-28

The four-test `marketing_delivery_persistence` target passed against a fresh
tenant write/read PostgreSQL pair. This includes callback inbox atomicity and
idempotency, channel-scoped claims, exact authoritative outbox projection,
durable cursor advancement, reset/rebuild, and source/read reconciliation.
The local provider/account, scheduler, and onboarding acceptance targets also
passed on fresh control databases. External provider certification remains
separate and was not claimed.
