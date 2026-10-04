# ADR-018: Modular AI and separately reviewed hosted commercial offerings

Accepted in the Rust era; modular ownership and human-reviewed AI remain the product direction. The original hosted stepper-seat proposal is superseded for this branch by [ADR-028](ADR-028-sandbox-cost-plus-support-billing.md) for historical Support billing and [ADR-037](ADR-037-sandbox-ai-gateway-and-rated-holding-billing.md) for the target Sandbox offering. It is not a live price or entitlement.

## Retained boundaries

Company/Holding source access applies to provider requests and the resulting proposals. BYOK and ERPX-funded usage remain distinct; a reviewed proposal cannot approve, post or pay an owner transaction. Hosted beta access never automatically becomes paid, and a first signup does not receive Platform authority. Customer configuration and usage are isolated from Platform control-plane billing evidence.

Current gateway meters, quota/cost evidence, Sandbox invoice approval, pricing assignment and retained historical invoices are defined by [Billing](../contracts/next/billing.md), [pricing feasibility](../contracts/next/billing-pricing-hypothesis.md), and the generated [catalogue](../contracts/next/catalogue.md). The earlier unlimited-AI and USD stepper examples are historical proposals, not current behavior. A production commercial offering requires separate pricing/capacity/provider/business acceptance.

The original Rust implementation and proposal remain recoverable through [the cutover references](../../CUTOVER.md). Their old workspace counts, Cargo paths and provider defaults do not describe the present 33-module catalogue.
