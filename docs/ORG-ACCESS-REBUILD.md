# Organization and access rebuild

## Outcome

The rebuilt organization has five departments, 14 department-owned hierarchy roles, no generic hierarchy roles, no system-admin role, and no active user identities.

| Department | Role | Rank | Primary system responsibility |
|---|---|---:|---|
| Information Technology | IT Manager | 3 | Platform administration, RBAC configuration, AI configuration, integrations, user access administration |
| Information Technology | IT Supervisor | 4 | Platform operations, AI and integration maintenance, audit and access-request intake |
| Information Technology | IT Officer | 5 | Monitoring, read access to platform configuration, AI operation, integration event review |
| Human Resources | HR Manager | 3 | Employee lifecycle, department and role assignment, organization ownership, access-request resolution |
| Human Resources | HR Supervisor | 4 | Employee creation and updates, leave decisions, quota administration |
| Human Resources | HR Officer | 5 | Employee records, leave intake, quota and directory operations |
| Accounting | Accounting Manager | 3 | Accounting authorization, settlement posting, GL posting |
| Accounting | Accounting Supervisor | 4 | Accounting approval, confirmation, PR and PO accounting review |
| Accounting | Accounting Officer | 5 | Accrual preparation, accounting verification, GL and ledger review |
| Financial | Chief Financial Officer | 2 | CFO and high-value authorization, finance override, executive reporting |
| Financial | Financial Manager | 3 | Finance authorization, budgets, PR and PO finance approval |
| Financial | Financial Supervisor | 4 | Disbursement authorization, payment and settlement supervision |
| Financial | Financial Officer | 5 | Payment preparation, payment evidence, finance queue operation |
| Executive | Chief Executive Officer | 1 | Final executive authorization and executive reporting |

Rank 1 is the highest authority. Manager, Supervisor, and Officer use ranks 3, 4, and 5. CFO is rank 2 and CEO is rank 1.

## Department relationship

`perm.roles.department_id` is the direct foreign key from every hierarchy role to `perm.departments.id`. The database rejects hierarchy roles without a department and rejects system roles with a department. User assignment must choose a role owned by the selected department.

The canonical department ids are `it`, `hr`, `accounting`, `finance`, and `executive`. The matching compatibility permissions remain `user:dept:<id>::allow`.

## Permission boundaries

Every role receives the authenticated baseline, personal expense and PR creation, its own expense visibility, directory access, organization visibility, and the shared inbox tiles.

Department permissions expose the relevant product area. Role permissions add authority by seniority:

- Supervisors can perform department verification and department approval.
- Managers inherit department verification and add department authorization, claim reassignment, and team management.
- Accounting Officer, Supervisor, and Manager own accounting verification, supervision, and authorization respectively.
- Financial Officer, Supervisor, Manager, and CFO own payment, disbursement, finance authorization, and CFO authorization respectively.
- CEO owns CEO authorization and executive approval.
- HR authority progresses from employee service, to leave and quota decisions, to employee lifecycle and access assignment.
- IT authority progresses from platform operation, to configuration maintenance and audit, to RBAC and platform administration.

IT Manager receives explicit administration grants. `admin:system:bypass::allow` is not granted to any role, so IT administration does not imply financial or executive approval authority.

## User cleanup

Historical business records reference the current users through protected foreign keys. Deleting those rows would either fail or require deleting expense, slip, procurement, HR leave, and journal evidence.

The migration therefore performs an audit-safe cleanup:

1. Delete every authentication session.
2. Delete every role, department, and individual-permission assignment.
3. Remove LINE identifiers.
4. Replace employee codes and names with stable archived values.
5. Clear position, job description, and department labels.
6. Deactivate every identity.

The result is zero active users and an empty access-assignment directory while financial history retains valid foreign keys. No replacement or mock users are created. A real first user must be created through the controlled bootstrap process and assigned a department-owned role before interactive administration resumes.

## Delivery sequence

1. Back up the database before applying the destructive migration.
2. Apply `db/2026-07-29-A-org-structure-rebuild.sql` in one transaction.
3. Run `db/2026-07-29-B-org-structure-assertions.sql`.
4. Verify role creation and assignment APIs enforce the department relation.
5. Run ESLint and TypeScript static checks.
6. Start the Next.js development server on port 3004.

## Recovery

The pre-migration database dump is the rollback source because anonymization intentionally overwrites identity fields. Restoring only access tables is insufficient when original employee names and codes are required.
