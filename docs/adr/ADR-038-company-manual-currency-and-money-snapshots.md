# Company manual currencies and immutable money snapshots

Accepted 2026-09-29 for ERPX business transactions. A Company starts with USD as its default currency for display and new transactions. Each person can choose a different display currency for that Company; the same choice applies in ERP, POS and PMS. An existing accounting chart keeps its functional currency, and historical records keep their original amounts. ERPX platform subscription billing is outside this decision.

## Decision

- Use the current ISO 4217 catalogue and its minor-unit precision. A Company must explicitly configure a precision before it transacts in an ISO code whose minor unit is unspecified.
- Store transaction amounts as integer minor units with the ISO code and scale. Exchange rates are positive, high-precision decimal quotes in the form `1 USD = N units of currency`, manually confirmed by a person with `finance.approve` for a Company and effective date. Revisions are append-only and attributed through the command audit log.
- Resolve a rate by the latest confirmed revision effective on or before the transaction date. When none is effective by that date, use the latest confirmed revision with a visible fallback label. When none exists, show the original amount and block a new transaction that needs conversion.
- Convert each amount with decimal/integer arithmetic, round once into the target minor unit, and only then aggregate. A confirmed or posted transaction retains its original amount, currency, scale, rate revision and USD equivalent as a snapshot; later rate revisions do not rewrite it.
- Keep an official receipt or accounting document in its transaction currency. A display-currency equivalent is explicitly labelled as reference information.
- POS and PMS use USD master prices when starting new sales or bookings, convert with the rate for that business date, and lock the price and rate on confirmation. Payment rails and mapped accounts must support the actual transaction currency.
- Payroll keeps the contractual amount and THB statutory and payout amount with their rate evidence. The existing production certification gate remains in force.

## Consequences

The currency selector is a preference and a default for new work, not a ledger re-denomination. Any module that stores a money amount must add currency, scale and rate-snapshot evidence to its write path before claiming support for a new currency. Existing two-decimal records retain their current interpretation. Cross-company rates and preferences cannot be shared.

The catalogue source is [SIX ISO 4217 data standards](https://www.six-group.com/en/products-services/financial-information/market-reference-data/data-standards.html). Updating that catalogue requires a reviewed data update and must not change the interpretation of previously stored money.
