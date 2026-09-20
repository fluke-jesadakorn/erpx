# Sandbox billing: reviewed actual costs plus Support

Accepted 2026-09-17. ERPX owns one Hosted service enrollment per Holding and its Bangkok calendar-month service periods. Stripe Invoicing owns issued invoices and payment outcomes. We deliberately use standalone invoices after administrator cost review rather than recurring Stripe Subscriptions, which could collect before costs are complete. This supersedes the seat-price and Stripe-subscription lifecycle decisions in ADR-007 and ADR-018 **for this Sandbox offering only**.

All modules remain available. Infrastructure Only has zero Support; Standard is THB 500/month and Priority THB 1,500/month, plus reviewed provider-funded costs without markup. User-funded BYOK is excluded. Support in the first month is prorated by inclusive Bangkok calendar days. Plan changes, Support cancellation and Hosted termination take effect at the next month boundary and can be withdrawn beforehand. Immutable plan revisions protect historical periods. Late costs retain the original service period and never restart Support.

Approval freezes line snapshots and queues a durable command. Stripe drafts use `auto_advance=false`, explicitly attached invoice items and stable idempotency keys. Finalization follows approval; manual invoices are due in seven days and automatic collection requires a saved Stripe card and explicit consent. Stripe owns collection retries; ERPX retries delivery of the same command. After an ambiguous operation exceeds the Stripe idempotency retention safety window, processing stops for reconciliation rather than risking duplication.

Signed test events enter a durable inbox before acknowledgment. Workers retrieve current Stripe state under the same Holding lock used by commands; stale event snapshots and browser return URLs cannot mark invoices paid. A periodic reconciliation repairs missed deliveries. Unpaid issued invoices can be voided and replaced; paid invoices stay immutable and adjustments reference the original. A net credit requires a separately reviewed credit note; there is no automatic refund.

This is Sandbox only: test keys and events, clearly labelled simulated costs, no actual workspace restrictions or data deletion, no tax, discounts, annual plans, ticket system, SLA, or automatic posting into customer Finance. Going Live needs a separate decision.

References: [Stripe Invoicing integration](https://docs.stripe.com/invoicing/integration), [invoice update constraints](https://docs.stripe.com/api/invoices/update).
