# Staff requests, levels and work

All paths below are relative to the selected workspace. `W` is `/holdings/{holdingSlug}`, `/holdings/{holdingSlug}/branches/{branchSlug}`, `/holdings/{holdingSlug}/departments/{departmentSlug}`, or `/holdings/{holdingSlug}/branches/{branchSlug}/departments/{departmentSlug}`. APIs use `/api/v1{W}`. The workspace resolves Company authorization; request placement comes from the requester's active Engagement.

An active staff account has exactly one active, currently dated employee or contractor Engagement in the Company and an unrevoked, unexpired Company assignment. Staff features do not require a particular selected business role. Missing legacy levels do not block requests. Multiple concurrent active Engagements are explicitly blocked until corrected.

## Interfaces

| Method    | Relative API path                                  | Interface                                                                                                                                                                  |
| --------- | -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET       | `/staff/people?departmentId=&levelId=`             | Use `levelId=unset` for unassigned levels. Company staff picker: stable `person_id`, `user_id`, `engagement_id`, placement and level; no compensation or unrelated records |
| GET       | `/staff/settings`                                  | Level catalog, department heads, staff picker and Company management permission                                                                                            |
| POST      | `/staff/levels[/{id}]`                             | `{code,name,rank,active,expectedVersion}`; catalog creation uses version 0                                                                                                 |
| POST      | `/staff/engagements/{id}/level`                    | `{levelId,expectedVersion,reason}`; records old/new levels and actor                                                                                                       |
| POST      | `/staff/departments/{id}/head`                     | `{personId,expectedVersion,reason}`; initially version 0                                                                                                                   |
| GET       | `/people?levelId=&departmentId=`                   | Existing scoped directory with level fields and filters                                                                                                                    |
| GET, POST | `/projects[/{id}]`                                 | `{title,description,memberUserIds,expectedVersion}`; creator owns project; detail includes members and visible tasks                                                       |
| GET, POST | `/staff-tasks[/{id}]`                              | `{title,description,assigneeUserId,dueAt,projectId,startApprovers,completionApprovers,expectedVersion}`                                                                    |
| POST      | `/staff-tasks/{id}/revise`                         | Full task configuration plus `{reason,expectedVersion}`; returns to draft and retains earlier approval history                                                             |
| POST      | `/staff-tasks/{id}/transition`                     | `{action,expectedVersion,note}`; `submit`, `start`, `save-progress`, `complete`, `approve`, `reject`, `cancel`                                                             |
| GET, POST | `/routine-templates[/{id}]`                        | Task configuration without due date, plus `published` and `expectedVersion`                                                                                                |
| POST      | `/routine-templates/{id}/clone`                    | Full template input; caller becomes the copy's creator                                                                                                                     |
| POST      | `/routine-templates/{id}/run`                      | `{dueAt}`; creates an independent task snapshot                                                                                                                            |
| POST      | `/routine-templates/{id}/schedules[/{scheduleId}]` | `{frequency,timezone,occurrenceTime,startDate,endDate,weekday,monthDay,dueOffsetHours,expectedVersion}`; saves paused; editing requires pause                              |
| POST      | `/routine-schedules/{id}/pause`, `/resume`         | `{expectedVersion}`; pause periods are retained                                                                                                                            |
| GET       | `/approvals`                                       | Company-authorized HR/owner route administration queue                                                                                                                     |
| POST      | `/approvals/{instanceId}/reroute`                  | `{expectedVersion,instanceVersion,approverUserId,reason}`; record and instance versions checked; current step only                                                         |
| GET       | `/staff/notifications`                             | Own updates filtered again by current record access                                                                                                                        |
| POST      | `/staff/notifications/{id}/read`                   | Marks only the caller's Company notification read                                                                                                                          |

Every POST and existing leave PATCH requires an `Idempotency-Key`. Record mutations, version assertions, approval decisions, audit and outbox records commit as one PostgreSQL transaction. Replayed requests return the committed result, changed inputs conflict, and competing versions cannot both commit. Outbox scope belongs to the record even when the acting approver uses another department workspace.

`dueAt` is an ISO datetime with an explicit offset. Approver arrays contain ordered account IDs with a maximum of 20 distinct approvers per phase. At least one phase must be configured. New forms default to completion approval. Project membership accepts up to 200 staff accounts.

## Request decisions

Existing expense endpoints remain `/expenses/{id}/{submit|approve|reject|cancel|post|reverse}`. Details include `approval`, `permittedActions`, and current approver/stage/blocking fields on `expense`. Receipt validation, immutable submitted receipts, accounting periods, independent posting, payables, reimbursement, reversal and bank controls remain in place.

Existing leave creation remains `/leave-requests`; PATCH `/leave-requests/{id}` saves or submits `{startDate,days,handoverNote,expectedVersion,saveOnly?}`. `saveOnly:true` retains a draft when routing is incomplete. Rejected requests can be corrected and submitted in a fresh cycle. POST `/approve` records only the current stage. POST `/transition` accepts `reject` or `cancel` with `expectedVersion` and `note`. Rejection requires a reason. The state remains `pending_approval` through the department stage and becomes `approved` after HR. Requesters can cancel draft, rejected or pending requests, including between department and HR decisions.

New employee requests use the latest published department flow, with one to twenty ordered person or Position selectors. When no flow is published, the current appointed head is the single department step. A head's own request goes to an eligible higher reporting manager or an independent CEO. Every Position selector resolves to exactly one active holder at submission. The department stages finish before HR, Accounting, Finance, Legal, or module owner review. Service requests that already route directly to the service team keep that route. HR review uses the current HR appointee when available; existing HR role review remains available for legacy organizations. Requesters and preparers cannot review their own submissions.

The policy version, selector, selected person, and ordered account IDs are saved on submission. New policy versions affect new requests. Current staff and position eligibility are rechecked at decision time; a pending step can be reassigned with a reason and audit record. A person higher in the selected approver's current reporting chain and Level can override only the pending department step, with a reason and both approver identities recorded. Later steps keep their saved order. Existing submitted requests retain their earlier routes.

## Tasks and participant access

Task states are `draft → start_approval? → ready → in_progress → completion_approval? → completed`. Start rejection returns to `draft`; completion rejection returns to `rework`. Resubmission retains decisions and starts a new cycle. Material revisions return to `draft`, clear the completion note, supersede approvals and require a new submission. Creators and project owners configure and cancel tasks; assignees execute and submit completion. Creators can review another person's completion when named, but cannot review their own start request. Assignees cannot review their own work.

Task access is limited to the creator, current assignee, configured or submitted approvers, project owner and project members in the same Company. Participation does not grant access to other departmental records. Expense/leave access uses claimant/requester and named approval participants, existing independently scoped Finance controls, and historical recorded decision makers. Only the current step exposes approve/reject actions.

Uploads accept `ownerType:"staff_task"` alongside `leave_request` and `expense_claim`. Download authorization resolves the owning record through these same policies. Task evidence is editable in draft by its configuration owner, or during execution/rework by the assignee. Upload finalization checks the current version and state and increments the owner version; previously created upload intents cannot mutate submitted evidence. Private-file checksums, media signatures, file limits and authorized downloads are reused.

## Routine calendar and operations

Published templates are readable and clonable by Company staff. Only the source creator or Company-authorized owner edits them. Manual runs use the caller as task creator. Project-linked manual runs require project membership. Each task retains the template ID/version, independent assignment and approval snapshot; later edits affect future generated tasks only.

Defaults: `Asia/Bangkok`, `09:00`, 24-hour due offset, paused schedule. Weekly weekdays use 0=Sunday and default to the start-date weekday. Monthly days default to the start-date day and clamp to month-end. Start/end dates are inclusive local dates. DST gaps move the occurrence forward and overlaps choose the first matching local time.

The durable PostgreSQL scheduled job handler recovers due active occurrences in bounded batches (20 examined occurrences per holding invocation). Paused intervals are skipped; backlog from active periods is recoverable after resuming. The next occurrence is generated even when previous tasks remain unfinished. Unavailable assignments or approvers produce blocked draft work rather than automatic approval. The schedule cursor, unique `(schedule_id,occurrence_at)` task key, task snapshot, notifications and outbox event commit together. Projections apply newer source versions and tolerate duplicate or delayed deliveries, using the current PostgreSQL outbox and job leases.

Structured operational logs expose `approval_route_blocked`, `routine_blocked`, `routine_generation_failed` and `routine_generation` with record/schedule Company scope and scheduling lag. Schedule details display the last generation error.

## Migration and release

1. Apply `0027_staff_work.sql` to both holding write databases before deploying the application or jobs process. Read databases require no additional migration.
2. Deploy the verified standalone Next.js/Node.js application and matching jobs process together, using the isolated release build (ADR-024). Completed legacy requests and their decisions are preserved; no historical HR decision is invented. Pending legacy requests start visibly blocked and initialize a route under the new rules once configuration is available, with audit evidence.
3. Configure levels and department heads. Legacy engagements stay “Not set”; activating or updating an active engagement requires a valid level. Assign a level before resuming normal engagement administration.
4. Review and publish routine templates, then explicitly resume schedules. New schedules are paused by default.

For local fixtures, `bun scripts/seed-staff-work.ts` adds explicit demo heads without changing existing heads, granting HR access or assigning legacy levels. The normal `seed:local` command includes this step. Browser verification uses local schedules and leaves them paused. Deployments and production schedule activation follow the release steps above.

Leave entitlements, benefits, meetings, team-balance calculations and advanced project dependencies remain outside this interface.
