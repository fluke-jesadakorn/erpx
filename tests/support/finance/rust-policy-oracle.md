# Finance Rust policy oracle

`fixtures/finance/rust-instruments/conformance.v1.json` records results from the TypeScript implementation retained at checkpoint `a5ed8737a2f05eff1ee9841fda4a094b3926d732`. Each source file has a SHA-256 identity. The generator refuses to run against edited oracle sources:

```sh
bun scripts/testing/fixtures/generate-finance-rust-goldens.ts
cargo test --locked --package erpx-server-finance
```

The native tests compare complete schedule rows, policy identifiers, exact decimal strings, calculation outputs and domain error messages. Schema failures require rejection without depending on Zod's diagnostic formatting. Cases include loan and lease present value, final settlement, anchored month ends and leap years, reviewed depreciation bases, units of production, signed half-away rounding, large intermediate integers, cash-flow controls, sequential material issues, invoice limits and the original journal preparation's functional-currency rounding. ECMAScript trimming and the legacy UTC handling of years below 100 are intentionally covered.

The journal preparation use case additionally exercises scoped fake ports. Authorization runs before facts; failed guards stop subsequent reads. It checks period and two-decimal currency policy, retained or explicit FX, active account identities and existing opening balances. Its sealed output retains the original actor and draft for an owner transaction. Production adapters must bind these ports to the caller's existing transaction and revalidate authority through the Identity boundary. Adapter errors remain distinct from semantic errors so serialization failures can propagate to the outer retry boundary.

The PostgreSQL writer requires an existing serializable transaction and rechecks Company/Holding scope, period coverage, active chart/accounts and opening balance conflicts. The sealed plan retains an opaque adapter witness for its database, backend and transaction; it cannot be deserialized from a wire DTO or reused in a later transaction on the same pooled backend. Disposable database tests use the unchanged Holding migration track and show that a journal and its lines commit with source/receipt work or roll back together when the receipt fails. The writer never opens a transaction or commits independently. These tests do not establish live IAM authorization or a complete Finance command workflow.

Allocation ties and manual-rate selection use pinned English ICU collation. A development oracle bundles the unchanged B0 algorithms and executes them on Node26.11.1/ICU78.3 with its recorded `en-US` locale. The corpus includes Thai, accented and canonically equivalent text, case, punctuation, numerical-looking identifiers and stable duplicate keys. Rust ICU4X behavior is compared to those source-runtime results; matching package version numbers alone are not compatibility evidence. Native tests additionally cover published versus Company-defined currency scales and six-decimal functional FX evidence. Image-size measurement and license packaging remain part of container preparation.

These portable calculation and application slices do not establish that the Finance owner has migrated. Real authorization, retained source evidence, database transactions, decision/version guards, journals, idempotency and HTTP/SSR wiring remain required before deleting the TypeScript implementation or enabling a Rust owner. The oracle is a development tool and is not part of the production service.
