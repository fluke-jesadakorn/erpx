These seven files are byte-for-byte source snapshots from commit
`8ece35be8900bd32efff2463eede39a24760680f`, before the Service Count writer
fence. They are loaded only by the isolated receipt helper and the four labeled
historical interoperability cases. Production code must not import them. The
loaders check SHA-256 before loading and rebase a fixed allowlist of imports
inside the historical graph. No database authorization or runtime fence is
changed by this oracle.

| Snapshot | Original path | SHA-256 |
| --- | --- | --- |
| inventory-counts.ts | packages/server-inventory/src/adapters/inventory-counts.ts | ab76f9ba70df7bd152646f803b2489c840bea3f00914d20e79e2e021951ccc06 |
| count-purchases.ts | packages/server-inventory/src/adapters/count-purchases.ts | 36616c99b0453d1a3bd89916edd1533a05eb3ee8b84246aa97fa16c0d605ead8 |
| service-initial-physical-stock-count-source.ts | packages/server-inventory/src/adapters/service-initial-physical-stock-count-source.ts | 8c8bf0709b1e827e404678fa408f86f4dc83374371bd50d2dc7039ebcc098a98 |
| flow-routing.ts | packages/server-workflow/src/adapters/flow-routing.ts | f9c8cfdded42dbb8844e9809de09c95e58fbb7e93bae0a7f6ed9141355ab0d6e |
| service-initial-physical-stock-count.ts | packages/server-inventory/src/adapters/service-initial-physical-stock-count.ts | 7049ef1e277dcb737313f032f0ee16657d2532cfd4b8b184619d19d7d25dd2c7 |
| service-initial-physical-stock-count-domain.ts | packages/server-inventory/src/domain/service-initial-physical-stock-count.ts | eca377fe539e8285769ea369549cae01545c49a4fe488042a9a5473c4bea08d1 |
| service-initial-physical-stock-count-source-sql.ts | packages/server-inventory/src/adapters/service-initial-physical-stock-count-source-sql.ts | 5c0052fad851b80fa136ed72d9d844f2b6b426012bbf26a6aeeb3dd84ae6095d |
