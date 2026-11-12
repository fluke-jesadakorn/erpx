# Maintenance notification corpus

Retained B0 row images for Maintenance notification derivation. Sixteen rows cover both exact
messages, Work owner recipient, Company/source/version/time, Unicode and the full 8/174/175/180
byte outer keys. No person/session fallback or notification Clock is introduced.

The TypeScript writer (`packages/server-collaboration/src/adapters/maintenance-notifications.ts`,
retired) produced the `sql`/`parameters` text now stored in `conformance.v1.json`. That module no
longer exists: the corpus is retained only as data, and the native Collaboration adapter
(`packages/server-collaboration/src/adapters/maintenance_notifications.rs`, exported from
`public/maintenance_notifications.rs`) owns the `staff_notifications` insert and the exact
notification/Inbox after-images. `retiredSources` records the deleted source identity, its
replacement and the removal condition. The retained `sql`/`parameters` fields are historical
statement text; nothing executes them.

`scripts/testing/fixtures/generate-collaboration-notifications-oracle.ts` still pins the deleted
source bytes and is owed to the testing-tools owner; until it is retired or rewritten, the corpus
must not be regenerated.

This is native data compatibility evidence. The native owner source is mandatory and has no
production default. PostgreSQL component tests use an explicitly synthetic source to verify the
actual 0312 Inbox trigger, private source binding, unit identity, rollback and final images. Actual
Maintenance/IAM/purchase/Workflow integration is a separate owner composition and is not
established by those synthetic ports. Preventive-plan notices are already written natively by
`packages/server-maintenance/src/adapters/preventive_plan.rs` (`insert_plan_notification`, unchanged
row shape); a public Collaboration port for plan notices and general Collaboration migration and
runtime activation remain pending.
