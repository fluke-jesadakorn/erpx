# Current Next.js contracts

[Production plan](../../production-plan.md) owns scope and priorities. Contracts describe current Next.js/Node.js/PostgreSQL behavior and explicit limitations; they do not certify production readiness.

- [All 33 modules: workflow/ownership/control catalogue](catalogue.md)
- [Service boundaries, shared transactions and isolated verification](service-boundaries.md)
- [Business readiness and release acceptance](business-readiness.md)
- [PMS](pms.md), [POS](pos.md), [Finance](finance.md)
- [People lifecycle](people-lifecycle.md), [Roster](people-roster.md), [Payroll](payroll-core.md), [Staff self-service](staff-self-service.md)
- [Identity/access](identity-access.md), [User management](user-management.md), [Active work context](active-work-context.md)
- [Staff work](staff-work.md), [Workplace UX](workplace-ux.md), [Messaging](messaging.md), [Calendar](calendar.md)
- [Executive remote MCP and external connections](executive-mcp.md)
- [HR/IT](hr-it-technology.md), [Billing](billing.md), [Legal](legal.md)
- [Attachments and record documents](attachments.md), [Web Push](web-push.md)
- [Employee/source Support reporting](support-general.md), [Marketing public intake](marketing-public-intake.md)
- [Sandbox billing pricing and feasibility](billing-pricing-hypothesis.md), [UCS lab boundary](ucs-lab.md)
- [Route table](route-table.md) is the route reference; handlers live under `src/app/api/`.

Follow [CONTRACT_POLICY.md](../../../CONTRACT_POLICY.md): update the affected contract and compatibility checks. Add migrations only when storage changes; no unrelated policy/schema/traceability edits. Preserve command idempotency and authoritative Company scope. Do not reuse fields or routes with different meanings. Breaking changes require an ADR and cutover entry.

- [People evaluation: Skills, Performance & Compensation](people-evaluation.md) — annual evidence, scoped comparisons and reviewed compensation.
