/** Native Support session reads are mounted on the Support read route and
 * consumed through `@erpx/feature-support/lib/native-support-read`. The former
 * TypeScript session queries (workspace, detail, context, evidence and work)
 * were removed; their oracle is `rust-support-ticket-read-postgres` and the
 * native inbox session target is `rust-inbox-support-postgres`. No runtime
 * TypeScript session reader remains. */
export {};
