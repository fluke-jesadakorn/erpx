# Native Field Service parts HTTP component

This contract describes the bounded, inactive native HTTP candidate for reserve-part and issue-part. It extends the [parts component](rust-field-service-migration.md) and [selected Identity ingress](rust-selected-ingress.md); it does not mount a route or activate a backend. The [production plan](../../production-plan.md) and owner register remain authoritative: all 41 business migration completion flags are false, and the TypeScript backend and Python runtime remain live.

## Interface and ordering

The public native router in [HTTP composition](../../../src/composition/src/field_service_http.rs), backed by [HTTP core](../../../src/composition/src/field_service_http_core.rs), exposes `POST /api/v1/holdings/{holding_slug}/field-service` when a caller explicitly mounts it. Its production factory uses actual Account cookie/header handling and a separately sealed Identity transport ingress, not a caller-created actor. The mandatory [OtherPost delegate](../../../src/composition/src/field_service_parts/http_transport.rs) owns other POST branches; this candidate qualifies only the joined reserve/issue branch. It does not supply a runtime mount.

[Administration](../../../src/composition/src/field_service_parts/administration.rs) preserves selected Identity and the provenance of real read, write and module errors. Schema normalization precedes real preflight and early receipt replay. For a fresh command, a read-only quote precedes late key validation and a second replay check. The body is captured once and memoized across an actual transaction retry induced by a once-only test trigger raising SQLSTATE `40001`; this does not prove organic contention. The retained transaction composes actual Support, Inventory, Finance Money and Collaboration effects with Workflow receipt and final source and authority guards; failures roll back the unit. The existing typed administration entry has its own previously documented ordering.

The logical request fingerprint retains original UTF-16 units, while PostgreSQL text binding uses the physical replacement target. The current schema's unconditional `result_json` JSONB cast in migration 1061 rejects a result containing a lone surrogate. The qualified actual HTTP test observes a fresh high-surrogate request return 500 with zero effects. A scalar U+FFFD request returns 200; changed high and low surrogate replays each return 409 and leave the saved hash, result and effects unchanged. A 220-character target returns 200. Historical replay after settlement withdrawal, inactive new-command refusal and missing-order precedence also pass. These are observations at the qualified source and schema, not a proposal to alter SQL triggers, schema or identifier normalization. The separate exact-expression SQL leaf observes `22P02`; it does not directly observe the hidden HTTP exception.

## Evidence and limits

The completed actual PostgreSQL HTTP qualification used the 55-path pre/post stable manifest `f6dece244feb6ab204292a21867f14182594dae30c77dd3ad48a79ec2bb7f2aa`: two registered HTTP functions passed, with zero failed, ignored or filtered; formatting and targeted strict Clippy passed. It covers retry and body capture, replay, quote and late-key ordering, rollback, genuine issue and IAM publication, the identity boundaries above, and history/withdrawal. This source includes subsequent test-only fixes to the earlier retained evidence, with no production deltas. Earlier actual Identity 17 and Directory 11 runs, and the retained Field Service 35, Workflow 16, Inventory 16, Finance 9, Issue Part 5 and Collaboration 6 runs are source-specific and overlap other evidence; do not sum them as unique coverage.

The 33 finite original fake-port observations use a different SQL-free profile: 17 return 200, one 401, two 403, one 404, ten 422 and two 500. Its lone-surrogate observation returns 200, distinct from the actual PostgreSQL HTTP 500 above. Source-derived parity includes 22 source observations, eight PostgreSQL leaf observations and three schema cases in one fixture; it is not the complete original TypeScript HTTP plus PostgreSQL transaction, which was not executed. The portable oracle comprises three generators and four fixture files. Promotion preserves every raw observation while relocating the generators and their provenance metadata. Source-derived expectations and actual qualified results have different scopes.

The registered catalogue IDs are `field_service.rust-http-parity.unit` and `field_service.rust-http-parity-postgres.integration`, associated with [unit](../../../tests/unit/field_service/rust-http-parity.test.rs) and [PostgreSQL](../../../tests/integration/field_service/rust-http-parity-postgres.test.rs) tests. Run the suite commands from the repository root with pinned Bun 1.4.3 and Node 26.11.1. For the oracle checks, use the shown timezone and locale; the generators guard this profile:

```bash
bun run test:unit -- --suite=field_service.rust-http-parity.unit
bun run test:integration -- --suite=field_service.rust-http-parity-postgres.integration
export TZ=Asia/Bangkok LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8
node --version  # 26.11.1
node scripts/testing/fixtures/generate-field-service-http-oracle.mjs --check
node scripts/testing/fixtures/generate-field-service-http-source-probes.mjs --check
node scripts/testing/fixtures/generate-field-service-http-schema-oracle.mjs --check
```

This branch includes the inactive library, portable fixtures and registered checks. Independent final component review accepts only this bounded scope; it does not authorize a live mount or complete the owner. Other POST behavior, complete JSON numeric and resource envelopes, host registry, SDK, SSR, frontend and runtime gates remain open. No full-route, owner or production acceptance follows from this component evidence.
