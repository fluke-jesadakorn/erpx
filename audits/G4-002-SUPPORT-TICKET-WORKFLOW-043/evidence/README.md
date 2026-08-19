# G4-002 Support Ticket Workflow Evidence

## Delivered

- `SupportTicket` now durably carries audience-tagged messages, assignment,
  escalation, and an SLA deadline through its existing JSONB aggregate payload.
- `SupportTicketCommand` provides bounded customer-reply, internal-note,
  assignment, escalation, SLA, and lifecycle-transition commands.
- Customer-visible reads use `SupportTicket::customer_visible_view`, whose
  message type has no internal-note field. Internal notes remain write-side
  aggregate data only.
- Assignment commands require the actor `HoldingScope` to contain the assigned
  scope. Assignment references, message bodies, escalation levels/reasons, and
  SLA horizon are bounded.
- Closed tickets reject all metadata/lifecycle mutations. Recurrence remains a
  new linked aggregate and does not reopen or mutate its closed predecessor.
- `AuthoritativeSupportService` exposes the metadata commands through the
  existing command/outbox unit of work. Create and recurrence IDs are derived
  from hashed idempotency material and ticket lineage; replayed command
  receipts rehydrate the stored aggregate instead of returning a fresh ID.
- `SupportPersistence::apply_ticket_command` atomically reserves idempotency,
  checks the expected version, rejects terminal tickets, and persists the
  aggregate payload. No migration was required.

## Verification

All commands were run from `/Users/fluke/Desktop/Work/folio/erpx` unless noted.

```text
cargo fmt -p erpx-domain -p erpx-application -p erpx-infrastructure -- --check
PASS

cargo clippy -p erpx-application --all-targets -- -D warnings
PASS

cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
PASS

cargo test -p erpx-domain support:: --lib
22 passed, 0 failed

cargo test -p erpx-application --lib
7 passed, 0 failed

cargo test -p erpx-infrastructure --test support_persistence
2 passed, 0 failed, 6 ignored

ERPX_SUPPORT_TEST_DATABASE_URL=postgresql://.../erpx_support_g4_002_20260724 \
  cargo test -p erpx-infrastructure --test support_persistence \
  support_ticket_metadata_commands_are_durable_audience_safe_and_replay_safe \
  -- --ignored --nocapture
1 passed, 0 failed

cargo check -p erpx-infrastructure
PASS; existing warning in commercial.rs for unused StripePriceResponse and
StripeProductResponse imports

git diff --check
PASS
```

The disposable PostgreSQL database used by the ignored test was dropped after
verification. No migration files were changed.

## Residuals and integration order

- Server/protobuf transport remains intentionally unchanged by this packet;
  customer-facing RPCs still expose the legacy ticket shape and must adopt the
  safe customer-visible projection in a later transport/UI packet.
- Production command composition must wire these typed commands to the
  `SupportPersistence` seam and emit the corresponding support event handlers.
- Support replies, notes, attachments, SLA timers, escalation workers, and
  notifications remain transport/worker/UI work outside the allowed paths.
- The broader G4 support approval/capability, incident, diagnostics, and
  customer-audit requirements remain open.

Integration order: domain contracts and customer-safe projection → application
authoritative command service → infrastructure atomic persistence → server,
worker, and client adoption in a later packet.
