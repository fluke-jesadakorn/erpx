# G6-G12-UI-014 — Platform integrations and settings read surfaces

## Objective

Connect `/platform/integrations` and `/platform/settings` to authenticated
server-owned read models while preserving the fail-closed, read-only boundary.

## Delivered behavior

- `ListPlatformIntegrations` requires an authenticated Platform scope and
  `platform.holding.read`, records the read, and returns categorical statuses
  for the control plane, identity, holding routing, object storage, and billing
  boundaries. It carries no endpoint, credential, token, database-key, or
  provider-payload material.
- `GetPlatformSettings` uses the same authorization and audit boundary and
  returns only architecture metadata: contract version, transport, physical
  holding isolation, aggregate authority, billing authority, storage authority,
  and command owner.
- WebAssembly gRPC-Web and native Tonic adapters are implemented for both
  reads.
- Dioxus surfaces preserve loading, denied, error, empty, and read-only
  states; no mutation or configuration control is exposed.

## Verification

- `cargo check -p erpx-server` — PASS.
- `cargo test -p erpx-console` — PASS; 22 tests.
- `cargo check -p erpx-console --no-default-features --features web` — PASS.
- `cargo check -p erpx-console --no-default-features --features desktop` — PASS.

## Open boundaries

These are read-only health/architecture projections, not provider onboarding,
secret-manager configuration, mutable feature flags, or integration repair
commands. Mobile target builds, provider live proof, owner seeding, true
PITR/WAL recovery, and broader G6/G12/G14 requirements remain open.

## Rollback

Revert the additive protobuf methods, server handlers, storage categorical
health accessor, and console adapters/components together. Existing route
scaffolds remain available and fail closed.
