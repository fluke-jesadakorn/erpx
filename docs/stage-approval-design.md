# Stage approval model

Expense, PR, PO, sales, and leave workflows use canonical stage keys and stage permissions. The current database definitions are in `db/schema.sql`; the role and department grants are in `db/seed.sql`.

## Authorization

Every actionable stage maps to a permission:

```text
stage:<stage>:act::allow
```

The stage catalog and aliases live in `lib/perm/stages.ts`. `lib/perm/chain.ts` checks the actor's effective permissions, and resource-specific policies in `perm.policies` add same-department or amount conditions.

For expenses, the normal chain is submission, department verification, department authorization, accounting verification, accounting supervision, accounting authorization, disbursement authorization, CFO authorization when required, CEO authorization when required, awaiting disbursement, and disbursed. Amounts below 200,000 THB skip executive authorization according to the policy rules.

Roles receive only their explicit operational stage grants:

- Department supervisors and managers handle department verification and approval.
- Accounting Officer, Supervisor, and Manager handle accounting verification, supervision, and authorization.
- Financial Officer, Supervisor, Manager, and CFO handle payment, disbursement, finance, and CFO authorization.
- CEO handles final executive authorization.

IT platform administration does not imply finance or executive approval authority. The baseline does not grant `admin:system:bypass::allow` to any role.

## Audit integrity

Every waybill transition appends a signed row to `folio.waybill_events`. The signature covers the sequence, event kind, stage transition, actor, previous event, and waybill ID. Update and delete privileges are revoked from the application and n8n roles when the schema is initialized.

The current UI and server actions operate on the shared Waybill object. There is no separate legacy approval matrix or L1/L2/L3/L4 module system.
