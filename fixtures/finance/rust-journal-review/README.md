# Native journal review component conformance

The frozen `conformance.v1.json` was captured on Node 26.11.1 from B0
`a5ed8737a2f05eff1ee9841fda4a094b3926d732`. It contains 42 actual journal-review
application cases and 133 actual reversal Money-lineage cases. Every source
blob SHA is recorded. The development generator loads those exact Git blobs,
including the old permission/module catalogue implementation: importing today's
generated permission projection would silently change the oracle.

The Money harness executes the unchanged private `ancestry`, `sourceMoney` and
`prepareJournalReversalMoney` functions from that blob. Synthetic database and
Money writer ports record the selected operation and immutable amount/rate
data; they perform no database work and prove no Company FX quote or purchase
authorization. A `CaptureNew` plan therefore has no invented USD amount/rate.
Only its genuinely new reversal is eligible for new current evidence. Existing
ancestry is never backfilled or retranslated. Negative immutable origins use
the dedicated exact signed-copy path, never the positive-allocation API.

Native properties additionally cover all 101 SQL-bounded ancestry depths,
every possible captured ancestor, exact units above JavaScript safe-integer
precision, two-currency ledger balance, and double reversal of recorded lines
and dimensions. Cross-Company, duplicate-UNIQUE and over-bound numeric fact
images are rejected as native admission guards; these are not claims that B0's
private helper accepted such impossible protected SQL query results.

`PgJournalReview` now implements the real borrowed PostgreSQL adapter. It uses
the public Workflow replay/execute protocol, exact Finance source/lineage images,
the existing Money reversal/carry allocation triggers, opening subledgers, and
the original projection-immediate/projection-deferred/all-immediate order. It
retains complete expected primary and secondary journal/line images, immutable
Money, opening invoice transitions and financial-event evidence. No internal
commit/retry, provider call or private Workflow query is introduced.

The adapter's Identity/People/Organization, Commercial, scheduled release and
foreign-owner source policy port is mandatory and has no production default.
Actual service composition, routes and TypeScript replacement remain pending.
Integration fixture authorities are explicitly synthetic; these tests do not
claim a complete Finance owner or assembled production authority. History is
available only after exact Workflow request-hash matching. Additional native
receipt admission binds returned IDs/status/version to the request-derived
Finance result. B0's bare replay could return an arbitrary stored result; the
tampered-result negative is an explicit native admission rule, not a claim of
literal B0 tamper parity. Frozen B0 expectations remain unchanged.

The configuration closure follows protected ancestry, actual related books,
candidate reversal period/date, affected charts/accounts/mappings and reporting
dimensions. Complete effective/retained rule rows are retained; earliest book
starts and secondary ownership use exact predicate projections. FX selection
retains the needed date/confirmation groups and all ID ties for the proven
native en-US collator. Broad utility table locks continue to fence absent and
newly inserted denial sources before the first snapshot. Unrelated Company
configuration and irrelevant historical/future rule bodies do not enter this
source image.

Each image admits at most 20,000 rows and 32 MiB of encoded source JSON across
all queries. SQL guards oversized rows and emits NULL markers after the
remaining byte budget; SQLx streams incrementally before decoding/collection.
No whole-image serialization or row-body serialization for sorting is needed.
These limits bound client source admission and transfer, not PostgreSQL server
heap, `work_mem`, statement execution, or total decoded Rust heap. Over-bound
relevant source images fail explicitly and the caller must roll back its unit.
No large-record throughput or full unbounded historical-data parity is claimed.

Real migrated disposable PostgreSQL tests cover post/reversal/Money/book/event
effects, exact History replay, signed reversal chains, legacy no-backfill,
opening invoice transitions, real updated-tuple 40001 whole-unit retry, audit
and deferred-constraint rollback, final source/authority rejection, a writer
committing before snapshot acquisition, compressed source limits, unrelated
configuration acceptance, date-effective books and earliest-opening denial.

Regenerate/check only as a development tool:

```sh
ERPX_NODE_BIN=/path/to/pinned/node bun scripts/testing/fixtures/generate-finance-journal-review-oracle.ts --check
cargo +1.99.1 test -p erpx-server-finance --test journal_review --locked
```
