# Commercial

Commercial manages the Hosted service agreement for a Holding, reviewed actual costs and fixed monthly Support. One account covers all Companies and Branches. The current implementation is the Sandbox offering in [ADR-028](../../../adr/ADR-028-sandbox-cost-plus-support-billing.md); archived Rust RPC descriptions and seat pricing are not the active contract.

ERPX owns enrollment, effective-month plan revisions, service periods, cost completeness, approvals and audits. Stripe owns invoice/payment state and stores payment details. A customer return URL never confirms payment. Infrastructure Only / Standard / Priority Support start at THB 0 / 500 / 1,500 per month plus actual costs. All modules are available regardless of tier. BYOK paid by customers is excluded.

Holding owners use Settings → Usage & billing. Platform administrators use Platform → Billing; auditors can read. Costs and evidence are isolated by Holding; evidence is private to Platform. Amounts are integer satang and periods use Asia/Bangkok. Missing cost review is not a zero-cost month. The first Support month is prorated by calendar day, while plan changes and termination are effective at month end and reversible until then.

The active control schema is `0009_billing.sql`. APIs are `/api/v1/holdings/{slug}/billing` and `/api/v1/platform/billing`, with Platform evidence under `/evidence/{id}`. Mutations require an expected account version, idempotency key and audit reason. The worker closes calendar periods, processes approved invoices and reconciles the durable Stripe inbox. There is no public clock override or early-month close action. Tests inject a clock into isolated fixture databases.

Draft costs may be corrected before approval. Approved invoice lines are immutable. Void unpaid invoices before issuing replacements; preserve paid invoices and add reviewed adjustments referencing them. Late provider costs use the original period, including after Hosted ends. Net credits stop for reviewed credit-note handling rather than creating negative invoices or automatic refunds.

Existing Stripe customers are reused. Active legacy test subscriptions block enrollment; use Stripe to resolve them. The old THB 100 Subscription Checkout is disabled. Portal handles cards/history; ERPX handles plan and Hosted lifecycle. Sandbox termination simulates effects without changing actual workspace access.

Verification: `bun --env-file=.env.local run test:integration tests/billing.test.ts tests/billing-api.test.ts tests/beta-auth-billing.test.ts`; opt-in Stripe Sandbox tests are documented in `docs/contracts/next/billing.md`.
