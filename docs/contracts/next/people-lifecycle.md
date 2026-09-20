# People: positions, access packages and lifecycle cases

The HR/access coupling and reviewed-only Directory flow below are historical. The [2026-09-18 HR/IT separation contract](hr-it-technology.md) defines the current placement, provisioning and Samba GPO behavior; legacy reviewed cases retain their original workflow.

Updated 2026-09-17. Local lifecycle and UCS directory preparation/write-back are implemented. The user explicitly deferred Windows host/RSAT/GPMC acceptance while retaining Windows compatibility and its UX model.

## Workspace

People has three primary views: People, Organization & positions, and Access & devices. Payroll remains under People. Settings collects templates and IT handoff, directory connections, custom attributes, roles, the access matrix, and reviews/history. Company IAM URLs redirect to these destinations. Platform IAM retains its separate routes and authority.

The People list includes HR profiles and accounts awaiting explicit profile linking. It does not infer a verified link from matching names or email addresses. IT can switch to the existing Explorer. Person and position details use the shared responsive side dialog; keyboard/button actions open the same lifecycle and preview flows as supported person/group/policy drags.

## HR → IT → individual review

1. HR creates a stable position with placement, staff level, reporting position, and work requirements. A database trigger creates one preparation task per position, assigned to the configured IT owner or the unassigned IT queue. Retrying a command or renaming a position never creates another task or a directory object.
2. IT drafts or publishes an access package. Published revisions are immutable; multiple positions can reference the same package. Publication and position binding require a fresh impact preview and optimistic version. The preview lists affected positions, occupants, and explicit computer targets. Preparation creates no individual assignments.
3. HR selects the person, engagement, position, actual manager, and effective time. The preview shows requested roles, groups, GPO references and preparation work. Missing position preparation, HR organization data or verified accounts results in a resumable waiting case.
4. A user with `iam.manage` requests individual access reviews under the existing MFA and delegation rules. An independent reviewer approves each grant; an empty ERPX role recipe still requires an activation review. The case becomes scheduled only after all approvals. Rejection blocks the case and revokes any partial assignments; HR can cancel and submit a revised request.
5. The durable worker applies due placement changes. Authorization also evaluates the case, effective time, immutable package revision, verified account link and current authority. A queued or future grant cannot be used early.

Package publication updates position bindings for future requests. Existing individual cases keep their reviewed revision: updating a shared package does not silently expand or replace existing personal rights. Use a new transfer/access review for current occupants.

IT can manage ERPX groups with `iam.directory.manage`; ERPX policy grants still require `iam.manage`. Group membership provenance distinguishes direct, automatic-rule, nested and effective position-package membership. Existing independent policy reviews are retained.

## Departure, suspension, transfer and return

- Offboarding and suspension create a Company/person cutoff in the same transaction as submission. `iam_principal_active` checks the effective time through every verified account alias. This blocks sessions, work contexts and authority-bound jobs even when the lifecycle worker is unavailable.
- The worker revokes Company assignments, invalidates selected work contexts, and ends or suspends the selected engagement. It preserves history and other Companies. IT handover, pending approvals, equipment return and directory completion remain independent tasks.
- A transfer ends only old lifecycle-sourced assignments. Independent direct/policy access remains subject to its own eligibility rules. OU changes are not inferred from HR placement.
- Rehire creates a new onboarding engagement. Old cutoffs remain effective until the independently approved new case is applied; previous grants do not reactivate.
- Cancelling is allowed before a scheduled case takes effect. After cutoff, restoration requires new access. Self-departure and protected recovery administrators are rejected; database guards also prevent promoting a person with a scheduled departure into protected administration.
- Non-beta legacy deactivation is redirected to the lifecycle workflow. Archiving requires ended engagements. Restoring an archived record restores its history, not its previous access.

## API

All writes use the existing authenticated work context, origin checks, command receipts, audit and outbox. Send an `Idempotency-Key`; optimistic versions and preview tokens are bound to the current Company and submitted definition.

Prefix: `/api/v1/holdings/:holdingSlug/people/lifecycle`.

| Endpoint suffix | Operation |
| --- | --- |
| GET `/` | Scoped positions, packages, cases, tasks and settings |
| POST `/packages/preview` | Publication impact and preview token |
| POST `/packages/draft`, `/packages/publish` | Versioned draft or immutable publication |
| POST `/position/preview`, `/position/bind` | Preview/bind a published package |
| POST `/cases/preview`, `/cases/draft`, `/cases/submit` | Preview, save or submit a lifecycle case |
| POST `/cases/:id/request-access` | Request independent personal reviews |
| POST `/cases/:id/cancel` | Cancel before effect |
| POST `/rehire` | Create a new engagement from ended employment |
| POST `/tasks/:id/complete` | IT acknowledges preparation work |
| POST `/settings` | HR templates and eligible IT owner |

IAM review decisions continue through the existing IAM API. Work and Inbox consume the same People lifecycle source, rather than introducing another Changes menu or routing lifecycle items through commercial document flows.

Migration `holding-write/0026_people_lifecycle.sql` adds eight tables, package/case membership functions, effective cutoffs, and position preparation/protected-administrator triggers. The scheduled worker calls `drainPeopleLifecycle` per non-beta holding; row locks, serializable transactions, bounded retry/backoff, stable IDs and per-case error tracking prevent duplicates and keep a failed placement from delaying unrelated departures.

Follow-up migrations `0027` and `0028` reopen the existing IT task when work requirements or placement change, preserve completion on title-only edits, and protect the HR primary-account link from attaching a recovery administrator to a departing person. New onboarding waits for IT to acknowledge revised requirements. Position editing and preparation share one detail panel.

## Directory preparation and write-back

UCS uses certificate-verified LDAPS for identities and UDM REST for account/group preparation. Inventories include immutable IDs, DNs, versions, supported fields, direct/nested members and explicit OU/computer targets. Draft/tested connections can read their inventory and preview work; submitting a write requires an active, independently reviewed connection, a tested schema, Company write enablement and an installed server profile that explicitly permits the operation and write subtree.

IT previews account creation, supported field edits, group creation/membership, account disable and account/computer moves. Account creation starts disabled with a generated password that never appears in responses or audit. Activation requires a verified person/account link and a completed, effective onboarding/transfer case. Directory password delivery/reset remains in the directory's own credential workflow. A departure-linked disable checks its effective cutoff; independent IT disable still checks self/recovery protection and whole-account impact.

Preview evidence covers referenced versions, affected positions, people, computers and Companies sharing the directory authority within the Holding. Disabling a whole account requires acknowledgement of every affected Company and a reviewer with authority over those Companies. The original operator and reviewer must remain authorized when the worker executes. Nested groups are checked for cycles and referenced group versions are revalidated at execution.

Commands enter existing Work/Inbox and independent IAM review. Durable operations use a per-connection lock, stable operation IDs, bounded transport timeouts, retries, audit/outbox and provider read-back. UDM creates use an operation marker, updates use If-Match, and an interrupted move can recognize the same immutable object at the reviewed destination. A conflict blocks work for a fresh preview. Completed jobs distinguish read-back from endpoint application. Departure profiles show ERPX cutoff separately from verified directory completion; manual preparation remains tracked independently.

Prefix `/api/v1/holdings/:holdingSlug/people/directory-operations`:

| POST suffix | Operation |
| --- | --- |
| `/refresh` | Read inventory and connector-supported schema |
| `/configure` | Configure tested write-back and per-field direction |
| `/preview`, `/submit` | Preview impact and request independent review |
| `/cancel` | Cancel work before its first external attempt |
| `/fields/preview`, `/fields/sync` | Preview/import owned fields or propose reviewed export |

Migration `0029_people_directory_operations.sql` adds directory state, durable operations, Holding templates and Company adoptions. The active schema now has 186 tables. Package-to-position drags, directory membership drags and explicit OU move drags open the same previews as their keyboard/mobile buttons.

## Owned fields and reusable settings

Custom fields support owner/viewer/editor roles for people, groups and organization units. Server-side snapshots hide inaccessible values and API mutations enforce field ACLs. Branch overrides are explicit; unit values inherit from the nearest Company-owned ancestor. Resetting an override retains required-field validation. External field mappings permit only schema-supported types and one direction per field. Import requires an explicitly verified identity and local field edit permission; export is a reviewed directory operation.

Holding templates contain onboarding/offboarding tasks and stable custom-field keys. Publishing a revision leaves existing Company copies unchanged. Adoption previews the current Company settings and field definitions, checks role availability and active group-rule dependencies, preserves field types/options/history, and retains existing individual grants and the IT owner. Endpoints live under `/people/templates` (`publish`, `preview`, `adopt`).

## Windows compatibility boundary

AD remains a distinct connector option. The finite Windows editor handles USB, inactivity lock and desktop policy definitions, with Drive/Printer Preferences compiled separately. An optional Windows RSAT adapter uses the same review/queue contract and observes linked OU scope and versions; it does not infer computer targets from a person's position. UCS account/group writes continue through UDM even when the optional runner is configured for Samba GPOs.

Windows write operations are not enabled in the current lab profile. Actual Windows/RSAT/GPMC and included/excluded endpoint tests are deferred by the user, not reported as passed. See the [Windows runner setup and acceptance contract](../../../ops/windows/README.md) and [UCS lab runbook](ucs-lab.md). GPO publication is not proof of enforcement on an endpoint, consistent with [Microsoft GPMC](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/group-policy/group-policy-management-console).

## Verification and development availability

The final integration suite passed 422 tests in 48 files, with the opt-in live provider test skipped in that run. The opt-in real UCS test passed after package installation finished (65 seconds): create/retry, update/conflict, membership, enable, move/retry and disable read-back. Typecheck, lint and the isolated release build passed. The final release build is recorded under ignored `.scratch/dev/people-release-build.log`.

The authenticated public browser verified the tested UCS LDAPS connection, UDM schema read, account-preparation impact preview, directory inventory and the mobile Holding-template/field editor. The lab connection was saved inactive; no employee grants or directory writes were submitted through the public browser. The real adapter test writes only isolated lab fixtures. Temporary viewport overrides were reset. The previously reported `ERPX_UNAVAILABLE` state did not recur on the verified People pages.

Development remains on `127.0.0.1:3100`, with its worker detached and logs/PIDs under ignored `.scratch/dev/`. `bun run dev` assigns one fresh deployment ID per run so public browser assets are refreshed between dev runs. It does not change production or the Tunnel origin. Public development currently receives a longer asset cache lifetime than the local response; restart dev after completing a change when verifying a fresh public bundle.

## Retirement and cross-department clearance (2026-09-17)

HR selects **People → person → Retire employee**, reviews the employment impact, confirms the last working day, timezone and access cutoff, and follows one case. Retirement is `offboarding` with `departureReason: retirement`; legacy definitions default to `other`. Existing preview/draft/submit/cancel endpoints remain authoritative. A scheduled date change requires cancellation before effect and a newly reviewed case. Rehire creates a new Engagement and independent access review; the old retirement remains historical.

The default cutoff is 00:00 the day after the last working day in `Asia/Bangkok`, editable before confirmation. The last working day remains the Engagement end date. Dates are not calculated from age. Nonexistent or ambiguous local times are rejected. Retirement effect, ERPX access cutoff, verified directory completion and clearance completion are distinct signals. Cutoff enforcement uses the existing authorization predicate, including all verified accounts for the Person in the Company, and does not depend on worker execution or checklist completion.

Migration `0040_people_retirement.sql` is additive. On submit, six preparation tasks are created atomically: HR records (creator), handover (eligible Reporting Manager), assets (HR-selected owner), directory (eligible configured IT owner), final payroll (HR-selected Payroll owner), and financial balances (HR-selected Finance owner). Missing owners remain visible in their department/HR Team queues. Tasks remain actionable after retirement; cancellation before effect cancels pending tasks. Existing self-departure, protected-administrator and Company boundaries still apply.

Clearance uses the existing Calendar `task.plan` and `task.complete` commands and transaction/receipt/version/audit/outbox machinery. All completion routes share the same authorization check. Completion requires the exact assigned principal and current department permission (`payroll.prepare`, `finance.write`, or `iam.directory.manage` for those departments), with Company scope for finance/payroll/directory. `not_required` requires a reason. Payroll/Finance `completed` requires source document references. Source access is checked at command time and again when evidence is read; unauthorized readers see a restricted-document label. Source status is read live, and completing clearance never approves, posts or pays a source document. The case page is `/people/clearance/:id` and source links return to the original workflows.

### Development examples

Run with the pinned local toolchain and development environment:

```sh
bun --env-file=.env.local run db:migrate:local
bun --env-file=.env.local run seed:retirement:local
```

The command refuses production, non-loopback PostgreSQL and databases outside the `erpx_dev_` namespace. It creates six unmistakably fictional people with stable `demo-retirement-v1-*` IDs and `example.invalid` addresses: draft; scheduled in 30 days; near retirement with unassigned handoff; effective with financial clearance pending; clearance complete; and cancelled before effect. Repeat execution skips existing examples without overwriting them and refuses colliding non-demo records. It creates no sign-in grants, sends no notifications/email and calls no directory/payment provider. Completed examples use an explicit fictional “not required” rationale.

Coverage lives in `people-lifecycle.test.ts`, `retirement.test.ts`, `calendar.test.ts` and `work-sources.test.ts`, including repeat submit/seed, cancellation, cutoff without worker, post-cutoff clearance and denied IT financial completion.
