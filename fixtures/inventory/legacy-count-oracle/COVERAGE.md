# Service Count historical interoperability coverage

The four `HISTORICAL COMPATIBILITY` cases in
`tests/integration/field_service/field-service.test.ts` execute byte-verified
8ece35be8 TypeScript source only as a historical reference. Their preserved
assertions do not establish that the current Rust route passed a journey.
The fixed Vite namespace resolves only the historical graph; ordinary imports
continue to use the current production modules and their writer fence.

| Historical case | Current Count evidence | Remaining distinction |
| --- | --- | --- |
| First physical Count, Files evidence, independent approval/post and later Service cost/billing | `tests/integration/field_service/rust-service-initial-physical-stock-postgres.test.rs`: selected Account create and exact replay, source/actor/Flow checks, bound Files evidence, independent approval, post, journal/stock effects, and exact create/post replay after a **synthetic** later POS movement. `tests/integration/field_service/rust-service-initial-count-update-postgres.test.rs`: actual live Files purchase boundary and tampered-witness rollback. | The later Service reservation/issue/billing chain remains a historical interoperability assertion here; it is not represented as a native end-to-end journey by these tests. |
| Stale or foreign selection, deferred master change, exact retry | `rust-service-initial-physical-stock-postgres.test.rs`: mismatched context and actual foreign Company product deny with no Count/receipt; late product master drift rolls back the master, Count and receipt; exact fresh create/replay succeeds after trigger removal. | The HTTP/browser journey and all historical scenario variants are not implied by this test. |
| Existing Supply mapping after withdrawal and a new absent-part Count | `rust-service-initial-physical-stock-postgres.test.rs`: selected first Count and posting against a synthetic retained Service mapping receipt and stock-history guards. The existing Commerce Service setup PostgreSQL suite covers the real setup owner separately. | The complete historical Supply withdrawal, retained mapping and new-part chain remains a historical interoperability assertion until a current native journey proves it. |
| Product entry under actual Finance or Stock assignment | Current production `serviceInitialPhysicalStockCountProductEntry` remains the Identity-owned entry dependency. `rust-service-initial-physical-stock-postgres.test.rs` checks selected Account authority and denies current roles lacking Count permission; the historical case retains its original entry assertions. | The historical case is not itself proof of current native entry routing. |

Count-specific checks spanning those cases map as follows:

- Create identity, one source and receipt, changed-body conflict, later-history
  replay, Stock and Finance effects, Files binding, independent approval/post,
  Flow assignment and replay: `rust-service-initial-physical-stock-postgres.test.rs`.
- Returned/rejected Count, subsequent cancellation, permitted history replay
  and current purchase/role denials: the same selected Account test's
  `native-reject-*` sequence. Receipt integrity tampering remains covered by
  that test and by `rust-stock-count-flow-postgres.test.rs`.
- Generic draft update and exact retained TypeScript update/Flow receipt replay,
  current Supply gate, live writer fence, and stale-snapshot birth races:
  `rust-service-initial-count-update-postgres.test.rs`.
- Real Service Files source boundary and tamper rollback, malformed and
  native-only source quarantine, and POS evidence preservation:
  `rust-service-initial-count-update-postgres.test.rs`,
  `service-count-native-writer-fence.test.ts`, and the targeted POS-only
  first Count case in `tests/integration/pos/pos.test.ts`.
- Setup initialization and confirm-existing receipts remain separate Commerce
  owner evidence. The Field historical test's full withdrawal/consumption
  assertions remain historical until the current native journey covers them.

The fresh browser and SQL evidence is owned by the delivery coordinator and
must be attached only after it actually passes. Production writer retirement
remains gated on that evidence and review of the full dependency chain.

The historical labels preserve the original case titles with one prefix. The
four original titles are not entries in the required-case catalogue:

| Original case title | Current test title |
| --- | --- |
| Service-only fresh parts setup independently posts the first physical spare and consumes it through original reviewed cost and billing owners | HISTORICAL COMPATIBILITY: Service-only fresh parts setup independently posts the first physical spare and consumes it through original reviewed cost and billing owners |
| Service first physical Count refuses stale and foreign selections and rolls a deferred real master-source fault back before an exact-key retry | HISTORICAL COMPATIBILITY: Service first physical Count refuses stale and foreign selections and rolls a deferred real master-source fault back before an exact-key retry |
| Service confirms an unchanged historical Supply mapping after normal Supply withdrawal and owns only a new absent-part physical Count | HISTORICAL COMPATIBILITY: Service confirms an unchanged historical Supply mapping after normal Supply withdrawal and owns only a new absent-part physical Count |
| Service product entry uses the actual current Finance or Stock assignment and named Count source without generic package authority | HISTORICAL COMPATIBILITY: Service product entry uses the actual current Finance or Stock assignment and named Count source without generic package authority |
