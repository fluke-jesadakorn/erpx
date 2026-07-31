# G14 local migrator deterministic ID-map validation

## Scope

This audit covers one local safety improvement in
`apps/migrator/src/main.rs`. It does not authorize production migration,
descriptor or vendor/device claims, command-owner changes, Folio retirement, or
cutover.

## Delivered behavior

The bounded `map` command now fails closed when:

- the source JSON object repeats a key, which would otherwise be silently
  collapsed by map deserialization;
- the deterministic mapper returns a different target for the same
  namespace/context/source tuple on immediate re-evaluation; or
- two source entries produce the same target UUID.

Existing path, size, secret, UUID, namespace, and read-only output guards remain
in force. A mapping is emitted only after the new validation succeeds.

## Changed paths

- `apps/migrator/src/main.rs`
- `audits/G14-MIGRATOR-ID-MAP-VALIDATION/README.md`

## Focused evidence

Source-level tests cover deterministic/context-bound mapping, duplicate source
key rejection, duplicate target UUID rejection, reconciliation explanation
handling, and secret/path rejection. The focused migrator unit run passed 5/5.
The full untouched migrator test target reported 62 total tests: 61 passed,
0 failed, and 1 ignored external-certification test.

Repository-required checks also passed: `bun run lint` reported 0 errors and 4
existing warnings in `infra/scripts/start-n8n.js`; `bunx tsc --noEmit` passed.

## Exact limits

This is local CLI validation only. It does not prove a live source/target
database migration, rollback rehearsal, external provider/device acceptance,
production safety, or cutover readiness.

## Blockers

No live migration or production cutover was attempted. Any test, lint, or type
check failure outside the changed migrator source is reported by the handoff
without being treated as evidence for this audit.
