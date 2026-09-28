# Staff self-service

Implemented 2026-09-17. Home keeps its feed. My shifts, My leave, Overtime and Purchase request are available in the holding-wide quick-access catalog in the desktop right rail; Home no longer repeats them in a fixed strip. On smaller screens, the quick-access shelf is a collapsed disclosure above page content, and My day starts collapsed. My shifts appears only when roster access is available. Its data request is independent of feed rendering; failed sources are identified rather than converted into empty totals.

## Routes and commands

All routes use the active Company workspace and normal authenticated work context. Mutations require the rendered work-context headers, an Idempotency-Key and, for existing records, the expected version. The existing branch/department URL rewriting also applies.

- `GET /api/v1{W}/self-service/summary`: next published shift, three actions, three recent active requests, setup blockers and unavailable sources. Its legacy `actions` field remains for compatible clients; the quick-access shelf reads the navigation projection.
- `{W}/people/roster?view=mine`: personal published schedule and existing swap, move and extra-slot requests. A separate overtime link prefills an assigned shift; roster approval does not approve overtime.
- `{W}/leave` and `{W}/leave/new`: balances, policies, holidays, requests and recoverable drafts. Existing request URLs remain `{W}/tasks/people_payroll/{id}`.
- `/api/v1{W}/leave-requests`: create/list (optional `state` filter); `/preview`; `/{id}` for detail/edit; existing `/approve`, `/transition` and `/cancellation-review` commands. Deprecated v1 day-count commands are normalized into the policy-calculated path, including nonworking dates.
- `{W}/overtime`, `{W}/overtime/new`, `{W}/overtime/{id}` and `/api/v1{W}/overtime-requests`: list/create, `/preview`, `/{id}` detail/edit/transitions. Actions are save, submit, authorize, actuals, verify, approve, return, reject and withdraw.
- `{W}/people/settings?section=leave`: Company leave types, multi-type policy versions, workweeks, calendars, assignments, grants/accruals/carryover, audited adjustments and explicit overtime eligibility. Assignment endings and eligibility changes have version checks. Ordinary staff get HR setup guidance here.
- Purchase creation stays at `{W}/work?new=purchase_request`; detail stays at `{W}/requests/{id}`. Its existing spending approval, ordering, receiving and accounting authority remains authoritative.

## Domain boundaries

Leave calculations use published roster assignments before the assigned policy workweek and holidays. Full/half-day segments, entitlement-year accounts, overlap checks, evidence requirements and eligibility are server validated. A range crossing policy assignments must be split into separate requests so each policy calculation can be reviewed. Submissions freeze their calculation/policy snapshots and reserve entitlement transactionally. Only final approval records usage; rejection releases reservations. Approved absence and usage remain effective until both independent cancellation reviews approve.

People settings maps each numbered Staff Level to a versioned Leave Policy recommendation with tracked annual and sick leave. Editing a default creates a new policy version and leaves past assignments and balance entries intact. A Position never grants leave directly; HR must accept the recommendation for an Engagement's effective-dated Leave Policy Assignment. A midyear level change schedules the new default on the next 1 January, while a custom assignment remains in force until HR changes it explicitly.

The append-only ledger drives the compatibility balances. Catch-up jobs enumerate missed accrual periods, annual/period grants and carryover with stable keys. No allowance is inferred from a Position. Additive migrations `0042`–`0048` preserve source-year balance provenance, legacy untracked policy assignments and historical requests; they add overtime records, immutable handoffs, explicit top-level engagement confirmation, purchase engagement links and settings versions. The final migration repairs missing self-service permissions on historical built-in roles through new revisions, preserving custom roles. The migration runner recognizes one exact previously applied development `0041` checksum and retains that receipt; all other checksum mismatches still fail.

Overtime has two paths:

1. Advance plan → reporting-manager authorization → staff actuals → manager verification → independent HR approval.
2. Retrospective actuals and late explanation → manager verification → independent HR approval.

Intervals include timezone and unpaid breaks, may cross midnight and cannot overlap another active claim. Future actuals are rejected. Returned actuals restart both reviews; staff can withdraw before final approval. Contractors need explicit eligibility. A missing reporting manager blocks submission unless HR has explicitly confirmed a top-level engagement and a distinct eligible Company owner can review it. HR remains independent.

Final approval creates one immutable Payroll Handoff, uniquely linked to the request/version. Manual overtime on any covered date must be reconciled first; the reverse path also blocks duplicate manual entry. Payroll preview snapshots identify the source handoffs and hours. Overnight contributions belong to their work-start date. No statutory payroll adapter, payment approval or production-payroll restriction was changed.

Private reasons and files remain on authorized records. Calendar displays “Unavailable” on calculated absence dates; it does not publish reasons or evidence. Work and Inbox use canonical request links and deduplicated durable notification rows. Policy-administration events are acknowledged without being mistaken for leave requests.

## Recovery and verification

Save draft confirms only after the server commits. Reopening the canonical record restores its saved values. Retry keys survive a failed request; stale-version recovery reloads the current version while retaining entered edits. Interrupted evidence uploads can retry their upload intent; after a browser reload the unfinished file must be selected again.

Verification is recorded in [staff self-service acceptance](../../testing/staff-self-service.md). This release does not seed new allowances or change real staff assignments, account credentials, Tunnel routing or the installed production service.
