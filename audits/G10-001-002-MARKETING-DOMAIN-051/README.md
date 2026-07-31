# G10-001/G10-002 Marketing and CRM consent domain foundation

## Delivered

The pure domain boundary now validates tenant-bound marketable profiles,
deterministic deduplication keys, purpose/channel/jurisdiction/policy-version
consent, pending double-opt-in and revocation, active suppression, bounded
audience membership, campaign lifecycle transitions, and send-time delivery
authorization with idempotent message replay/conflict detection.

## Independent verification

- `cargo fmt --all` — PASS.
- `cargo clippy -p erpx-domain --all-targets -- -D warnings` — PASS.
- `cargo test -p erpx-domain` — PASS: 91 tests, including four marketing
  consent/suppression/lifecycle/scope tests.

## Residuals

SQLx persistence, channel adapters, templates/assets, journeys and n8n
actions, CRM handoff, lead scoring, attribution/ROAS, custom domains, public
landing pages, worker scheduling, UI, retention/erasure jobs, and Finance
reconciliation remain open. This is not a complete Marketing/CRM module.

## Security and rollback

Suppression and consent are evaluated at delivery time, tenant scope is checked
before campaign delivery, and duplicate idempotency keys cannot send a second
message. The domain stores no credentials or provider tokens. Rollback is a
source-only revert until marketing persistence migrations are introduced.
