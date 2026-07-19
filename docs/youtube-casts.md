# YouTube Series — Cast & Flow Map

Trimmed demo footprint. Each episode uses one persona + one flow. Six personas, four departments, eight tiles in active rotation.

## Active personas

| ID | Name | Dept | RBAC | Episodes |
|---|---|---|---|---|
| 1 | John Staff | Development | L2A | 1, 2, 5 |
| 4 | Emily Manager | Finance & Account | L3 | 2, 4 |
| 15 | Charles Executive | Executive | L4 | 2, 4, 5 |
| 20 | Alex Admin | IT | L4 | 3, 5 |
| 27 | Andrew Supervisor | Finance & Account | L2A | 2, 5 |
| 29 | Brian Admin | Executive | L4 | 3, 5 |

Sign in via the persona menu (top right). Removed users return `user not found`.

## Departments

| Group | Users | Notes |
|---|---|---|
| Development | John Staff | Cross-department reporting to Andrew (Finance) |
| Finance & Account | Emily, Andrew | Heaviest transactional dept |
| Executive | Charles, Brian | Cockpit + CEO override |
| IT | Alex Admin | RBAC matrix, AI settings |

## Episode map

| # | Title | Persona | Flow | Tiles |
|---|---|---|---|---|
| 1 | Upload + Extract | John Staff | Drag PDF/PNG → OCR → expense draft | `/submit-expense` |
| 2 | Approval Ladder | John → Andrew → Emily → Charles | Submit → supervisor → head → exec | `/approve-expense` |
| 3 | RBAC Matrix | Alex Admin | Edit cell, see cascade | `/org-chart`, `/settings` |
| 4 | Override + Ledger | Charles | CEO override → journal posting | `/override-queue`, `/ledger` |
| 5 | Notifications | John, Andrew, Emily | Submit + approve + see bell | `/summary` |

## Cross-slice summary tile

`/summary` shows activity counts sliced four ways:

- **By Feature** — row counts per table (expenses, slips, PRs, notifications, etc.)
- **By Department** — users + expenses + slips + approvals per dept group
- **By RBAC Group** — role coverage per module group
- **By Persona** — per-user activity across all features

Gated by `tile-summary` permission (currently `L4 read`). Useful for the "show me everything" shot.

## Removed for the series

| ID | Name | Original dept | Why removed |
|---|---|---|---|
| 2 | Sarah Approver | Engineering | 0 transactions |
| 3 | Mark Reviewer | Finance & Account | Activities reassigned to Andrew |
| 5 | Olivia Director | Executive | Slip activity reassigned to Charles |
| 16 | Lisa Staff | Sales | 0 transactions |
| 17 | David Approver | Sales | 0 transactions |
| 18 | Robert Reviewer | Finance & Account | Activities reassigned to Andrew |
| 19 | Karen Staff | Marketing | Expense reassigned to John |
| 21 | Daniel Accountant | Finance & Account | 0 transactions |
| 22 | Michael Manager | Operations | 0 transactions |
| 24 | Patricia Manager | Human Resource | 0 transactions |
| 25 | Jennifer Staff | Human Resource | 0 transactions |
| 26 | Steven Supervisor | Engineering | John's `reports_to` reassigned to Andrew |

Dropped department groups: `dept-engineering`, `dept-sales`, `dept-marketing`, `dept-operations`, `dept-hr-2`. Plus orphan `dept-finance` and `dept-hr` from earlier rename.

## Reassignment rules

| Table.column | Source user | Target |
|---|---|---|
| users.reports_to_user_id | 26 (Steven) | 27 (Andrew) for John (id=1); NULL for others |
| approval_logs.actor_id | 2, 3, 18, 19 | 27 (Andrew) |
| slips.uploaded_by | 5 (Olivia) | 15 (Charles) |
| expenses.submitter_id | 19 (Karen) | 1 (John) |
| approval_policies.created_by | any removed | NULL (audit only) |
| departments.head_user_id | any removed | NULL |
| access_requests.target_user_id | 24 (Patricia) | NULL |

Auto-handled by FK: `notifications.user_id` (CASCADE), `domain_events.actor_id` (SET NULL), `ai_invocations.actor_id` (SET NULL).

## Recovery

```bash
# Restore from pre-cull snapshot
psql -h localhost -U contract -d folio_db -c "DROP DATABASE folio_db;"
createdb -h localhost -U contract folio_db
psql -h localhost -U contract -d folio_db -f /tmp/pre-youtube-cull.sql
# Reinitialize only a fresh database with db/setup.sh.
```
