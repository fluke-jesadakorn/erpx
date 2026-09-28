# Calendar and people planning

Calendar is available at `{workspaceBase}/calendar` alongside Home, Work and Inbox. The selected Company and workspace are resolved by the existing authorization layer. Hierarchy selection only narrows results; it never grants access.

## Reads and navigation

`GET /api/v1{workspaceBase}/calendar` accepts `date`, `view=day|week|month|agenda`, `hierarchy=organization|reporting`, `mode=mine|team`, `person`, `node`, `search`, `layers`, `timezone`, `cursor`, and `limit` (1–200). Date windows are one local day, one week, one month, or 30 agenda days. Pagination uses stable source identity and offsets; coverage describes the returned page. Clients label partial results and can load subsequent pages. Refreshing restarts pagination against current source state.

Day view places people on rows against the complete local day in the selected display timezone. Its ruler therefore contains 23, 24, or 25 actual hours as required by DST, and repeated hour labels include their UTC offset. Timed records are clipped visually at day boundaries without changing their stored duration, overlapping records use lanes, and all-day records use a compact click-only rail.

`GET .../calendar/hierarchy` paginates people independently (up to 200 per page), preserves organization parents and manager person IDs, and deduplicates engagements. Authorized lifecycle readers can see future joiners and ended engagements linked to lifecycle cases. Organization navigation is bounded to 5,000 nodes and fails explicitly above that limit.

`CalendarItem` is a read model, not a second event store. Separate adapters query roster shifts, leave requests, preparation tasks and lifecycle cases, appointments, and announcements. Reads do not initialize approvals or execute lifecycle transitions. Failed adapters return explicit source errors, not an empty successful source. Timed intervals retain their source timezone; all-day intervals use separate date fields and an exclusive end date. New appointments default to Asia/Bangkok. Nonexistent local DST times are rejected; PostgreSQL resolves ambiguous local times using its timezone rules.

## Authority and privacy

- Existing roster rules remain authoritative, including staff access to published department shifts, independent approval and published-change reasons. The shared roster workspace and extracted shift form are embedded in Calendar.
- `calendar.availability.read` gives existing built-in roster-reader roles absence dates and Busy intervals. It exposes no leave reason, document, guest details, or private appointment title. Pending leave appears only to existing request participants. Leave actions open the existing leave workflow.
- Active staff can organize their own appointments. `calendar.appointments.manage` allows delegated scheduling in the selected scope. Participants can read their appointment details. A scoped manager cannot see private details merely because an attendee, rather than the organizer, belongs to that scope.
- `calendar.announcements.publish` is granted to Owner, HR, People manager and Operations manager. Published Company, Branch and Department notices use the same source adapter and audience predicate in Calendar and the Home feed. Publication, edits and withdrawal require the current version. Notices addressed to a workspace's ancestors remain visible there.
- HR (`organization.manage`) and Company IT may plan ordinary lifecycle tasks. Only Company IT may plan privileged tasks. Named active staff may complete ordinary checklist tasks. Position preparation continues through access-package binding. Completing any checklist task does not grant access, delay a cutoff or certify a directory operation.

## Writes and migration

`POST .../calendar`, and dedicated `.../calendar/appointments`, `.../calendar/announcements`, `.../calendar/tasks` command endpoints, require an Idempotency-Key and the normal work-context headers. Appointment/announcement list endpoints are also available. Commands validate origin, Company, current permissions, placement, participant eligibility and optimistic versions. Writes, history, authority guards, command receipts, audit and outbox events commit in one serializable transaction. Retries reuse the receipt; concurrent stale edits fail.

Appointment commands: `appointment.save` (create or reschedule) and `appointment.status` (`cancelled`, `completed`, `no_show`). Completion and no-show require the start time to have passed. Overlap and approved-absence warnings reveal no conflicting appointment details; a meeting during an assigned shift is allowed. External guests are staff-managed contact records in the appointment, without ERPX accounts or outbound invitations.

Announcement commands: `announcement.save` and `announcement.status` (`published`, `withdrawn`). Visible dates are inclusive in the form and exclusive at the end in the read model. Home references the same announcement ID; no social-post copy is created.

Task commands: `task.plan` (optional due date, explicit assignee and responsibility) and `task.complete`. Migration `0038_calendar.sql` retains every existing assignee, defaults existing tasks to privileged and leaves deadlines null. Newly generated ordinary handover/equipment tasks may be delegated; directory, access, drive/printer and GPO work stays privileged. Unscheduled and overdue tasks remain discoverable. Last working day, ERPX cutoff and verified directory operations are separate items.

Calendar movement uses narrow commands which never require clients to resend unrelated or sensitive record data: `appointment.reschedule` changes local start/end and timezone, `announcement.reschedule` changes its inclusive visible date range, `task.reschedule` changes the due date, and `shift.reschedule` changes local date/start/end with a reason. Each requires the current record version and an idempotency key. `CalendarItem.permittedActions` advertises availability; shift details say whether a published week requires a reason.

Day-view appointment and shift bars move or resize in 15-minute increments with a 15-minute minimum. Week and Month move appointments and shifts to another date while preserving duration, move task due dates, and move announcement ranges while preserving inclusive length; announcement edge handles resize the range. Agenda and Day all-day rails remain click-only. Cross-row movement never changes participants or assignments. Escape and pointer cancellation abandon an active gesture. Successful ordinary changes save immediately; failures restore the original position. Published shifts hold their proposed position only while the user supplies the mandatory audit reason, and cancelling restores the source position. All existing scope, privacy, status, future-shift, same-week, absence, overlap, DST, history, audit, outbox and notification rules continue to apply.

External invitations, public booking, provider synchronization and resource booking remain outside this increment.

## Personal scheduling and recurrence (2026-09-26)

My Calendar Day and Week use a timed grid with an all-day area. Dragging an empty 15-minute slot opens the appointment form. Timed appointments can move or resize from either edge in 15-minute steps with a 15-minute minimum; the form remains the keyboard equivalent. Team and Coverage retain people rows. “New task” opens Work task creation with the selected date.

`appointment.save` accepts an exclusive all-day end date, account attendees with required or optional attendance, and a daily, weekly, monthly, or yearly recurrence rule. Account attendees can exist without an HR Person. A series expands only in the requested date range; occurrence keys use the scheduled local start, and exceptions preserve stable identity after moving or cancelling one occurrence. `appointment.reschedule` and `appointment.status` accept `occurrenceId` and `scope=single|future|series`. Splitting a series leaves past responses and history on the original. Nonexistent local DST times are skipped; recurrence retains the local wall-clock time across offset changes.

`appointment.respond` records Accept, Tentative or Decline for a series or one occurrence. Pending invitations are tentative for availability; declined invitations leave My Calendar. `appointment.reminder.set` stores each attendee’s preference (default 15 minutes, nullable for off). The durable scheduled worker publishes only due reminders for current, uncancelled, nondeclined occurrences. Creation, changes, removal, cancellation and responses publish staff Inbox events in the source transaction. `GET .../calendar/availability` takes selected same-Company `userId` values and a bounded `from`/`to` interval and returns busy intervals only. Conflict and approved-leave warnings do not prevent saving.

## Verification

`tests/calendar.test.ts` covers source privacy, Company/workspace boundaries, participants, delegates, hierarchy identity, future/ended engagements, independent cutoffs, ordinary and privileged preparation, all-day leave, overnight shifts, DST validation, pagination, overlap warnings, idempotency, concurrent edits and explicit source failures. `tests/calendar-api.test.ts` checks direct request validation, origin, permissions, defaults and cache policy. Existing roster, lifecycle, staff-workflow, inbox/navigation and migration suites provide regression coverage. Browser checks require normal authenticated sessions; authentication is never bypassed.

Verified on 2026-09-17: 103 tests passed across 11 calendar, roster, lifecycle, staff-work, migration and navigation suites; typecheck, lint and the isolated release build passed. Authenticated public browser checks covered the weekly grid, month overview, reporting hierarchy, search, appointment/announcement forms, event history, embedded roster and task planning. A 390px viewport defaulted to agenda and displayed the mobile form and detail panel correctly; Escape closed dialogs. Local and public health returned HTTP 200.

Development static bundles now send `Cache-Control: no-store` plus CDN cache headers from `next.config.ts`; the public response was verified as Cloudflare `BYPASS`. Production asset caching is unchanged. This prevents development chunk URLs from retaining earlier UI code across edits, following [Cloudflare CDN cache-control precedence](https://developers.cloudflare.com/cache/concepts/cdn-cache-control/).

## Presets and return context (2026-09-17)

`preset=mine|team|coverage` maps onto the existing query model. Coverage uses the team week with roster/leave layers; ordinary calendar grids show people with returned events first, while Coverage retains people without shifts. Layers, timezone and hierarchy controls live in Filters; active filters remain visible as chips and organization branches start collapsed. Changing a preset/filter/date adds browser history. Event links retain the query context, and Back/Forward restores filters and event selection. Embedded roster planning carries the selected department, week and event date.

Retirement last working day, timed cutoff and clearance due dates are read from their original People records. Assigned clearance tasks open the case page for a reason/evidence-based result; the Calendar quick-complete control is not offered for those tasks. Existing ordinary/privileged preparation rules remain in effect for non-clearance tasks.
