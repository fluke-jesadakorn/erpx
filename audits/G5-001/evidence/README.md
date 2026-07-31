# G5-001 / G5-004 / G5-005 commercial domain evidence

## Delivered packet

- Added an explicit commercial catalog shape for base fees, modules, setup fees,
  flat/seat/company/branch pricing, one-time/recurring/metered charges,
  monthly/yearly intervals, currencies, trials, coupons, included usage, and
  overage pricing.
- Catalog drafts validate required commercial invariants before producing a
  published revision. The published revision exposes read-only accessors and
  cannot be edited or republished through the domain API.
- Added an explicit `GrandfatheredPriceMigration` record linking a customer and
  subscription to ordered published revisions, old/new prices, effective time,
  reason, and recording actor.
- Added a billing lifecycle with explicit events, a half-open seven-day grace
  boundary, recovery, cancellation, and declared read-only capabilities.
- Added an entitlement allocation book that reserves purchased seat/company/
  branch capacity, prevents over-allocation, requires company activation before
  branch activation, and authorizes only activated company/branch scope.

## Verification

Run from `erpx/`:

```text
cargo fmt --all --check                         PASS
cargo test -p erpx-domain                      PASS (34 tests)
cargo clippy -p erpx-domain --all-targets -- -D warnings
                                                  PASS
```

Focused tests cover commercial pricing validation and published immutability,
grandfathered migrations, exact grace-period boundaries, read-only allowances,
and allocation/activation capacity limits.

## Scope and follow-up

This packet intentionally contains no Stripe client, webhook, persistence,
protobuf, server, UI, or runtime changes. Stripe synchronization, immutable
revision persistence, webhook reconciliation, entitlement projection, billing
authority integration, and production allocation commands remain integration
work for the dependent billing/foundation packets.
