# POS separates bills, fulfillment and provider settlement

POS uses existing Commerce stock and Finance ledgers, with each local business command committed in one serializable PostgreSQL transaction. A fulfillment survives bill splitting and merging so dispatched food never consumes ingredients twice; reservations are enforced at the shared stock write boundary so legacy Sales, transfers and counts cannot consume stock reserved by POS.

Company-owned Stripe Connect Accounts v1 use direct charges, Full Dashboard and company-paid Stripe fees. OAuth supports existing accounts; ERPX subscription billing remains separate. Provider operations run through durable leases and immutable account/mode bindings because an external payment cannot participate in the database transaction. A verified provider receipt can therefore remain pending internal posting, with explicit accounting recovery rather than repeating the charge.

Sources: [direct-charge merchant of record](https://docs.stripe.com/connect/merchant-of-record), [controller properties](https://docs.stripe.com/connect/migrate-to-controller-properties), [Accounts v2 OAuth boundary](https://docs.stripe.com/connect/accounts-v2).
