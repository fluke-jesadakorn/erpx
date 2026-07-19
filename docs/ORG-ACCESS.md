# Organization and access

Folio's organization is a five-department hierarchy with 14 department-owned roles.

| Department | Role | Rank | Responsibility |
|---|---|---:|---|
| Information Technology | IT Manager | 3 | Platform, RBAC, AI, integrations, and access administration |
| Information Technology | IT Supervisor | 4 | Platform operations, configuration, audit, and access intake |
| Information Technology | IT Officer | 5 | Monitoring, AI operation, and integration event review |
| Human Resources | HR Manager | 3 | Employee lifecycle, organization ownership, and access resolution |
| Human Resources | HR Supervisor | 4 | Employee updates, leave decisions, and quota administration |
| Human Resources | HR Officer | 5 | Employee records, leave intake, and directory operation |
| Accounting | Accounting Manager | 3 | Accounting authorization, settlement, and GL posting |
| Accounting | Accounting Supervisor | 4 | Accounting approval, confirmation, and procurement review |
| Accounting | Accounting Officer | 5 | Accrual preparation, verification, and ledger review |
| Financial | Chief Financial Officer | 2 | CFO authorization, finance override, and executive reporting |
| Financial | Financial Manager | 3 | Finance authorization, budgets, and procurement approval |
| Financial | Financial Supervisor | 4 | Disbursement, payment, and settlement supervision |
| Financial | Financial Officer | 5 | Payment preparation, evidence, and finance queue operation |
| Executive | Chief Executive Officer | 1 | Final authorization and executive reporting |

`perm.roles.department_id` connects every hierarchy role to `perm.departments.id`. `perm.user_departments` records a user's department, and `perm.user_roles` records their role. Assignment APIs reject a role owned by a different department.

Department grants expose product areas to the department. Role grants add authority by seniority. Direct user grants are reserved for explicit permanent or time-bound exceptions.

The canonical database seed creates the departments, roles, permission catalog, grants, tiles, policies, AI catalog, webhook providers, and chart of accounts. It intentionally creates no users or department heads. This avoids shipping mock employees or credentials and keeps the first-user bootstrap one-time and auditable.

To initialize a fresh database:

```bash
bun run db:setup
```

Then set `FOLIO_BOOTSTRAP_TOKEN` and call `/api/auth/bootstrap` to create the first IT or HR access administrator. That endpoint is disabled after the first active identity exists.
