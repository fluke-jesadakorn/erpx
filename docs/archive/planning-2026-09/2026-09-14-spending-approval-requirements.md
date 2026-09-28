# People management, organization structure, messaging, and spending approvals

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


**Status:** Application implementation added on 2026-09-14 following the approved requirements. Development migrations are applied. Existing staff must still be explicitly mapped by HR before new spending submissions become ready; no seniority is inferred from roles. Automatic position-based role grants and bank transfers remain future work.

## 1. Summary and delivery order

HR setup precedes the new spending approval workflow:

**Organization structure → positions and levels → staff, managers, and roles → approval readiness → spending workflow**

Provide both an organization chart and organization-based messaging. Build reusable positions and organization templates in the first application phase. Automatic assignment of system roles from positions remains a future phase.

The audience is HR administrators maintaining staff, company administrators configuring approval policies, staff submitting requests, organizational supervisors, Accounting, and Finance/Treasury. Vocabulary is defined in the [domain glossary](../../../CONTEXT.md).

## 2. HR onboarding and organization chart

### Staff levels

Every active staff member must have one assigned level. Smaller numbers mean greater seniority; titles are editable.

| Level | Default title |
| --- | --- |
| 1 | CEO |
| 2 | Executive / Director |
| 3 | Branch / Department Manager |
| 4 | Supervisor |
| 5 | Senior Staff |
| 6 | Staff |

Levels, job positions, and system roles are separate concepts. A level or reporting relationship does not itself grant application permissions. Existing staff must be explicitly mapped into this numbering convention; never infer seniority from access roles or silently reinterpret existing numeric ranks.

### Organization and positions

- HR manages company-scoped positions within branches and departments. Each position identifies its title, default level, and expected reporting position.
- Positions exist independently of their occupants, allowing vacancies and staff replacement. Multiple staff may occupy the same position type.
- HR can reuse organization templates containing positions, level defaults, and reporting relationships. Applying a template requires reviewing the resulting structure.
- Templates do not grant system roles in this phase. Position identities remain stable so future role bundles can refer to them.
- A position describes the expected structure; each staff member still has one explicitly assigned actual manager. The chart and approval routing use those actual reporting relationships.

### Adding and managing staff

- HR adds a person, records employment details, selects organizational placement and position, confirms the level, and assigns one reporting manager.
- Selecting a position supplies the default level and helps identify suitable managers. Do not calculate the level simply as “manager level + 1.”
- A manager is required before activating a staff profile, except for the explicitly designated CEO position. A senior title, access role, or missing manager does not independently establish this exception.
- Managers must be active in the same company and strictly more senior. Prevent self-management, circular reporting, and invalid placement.
- Allow incomplete onboarding drafts. Staff profiles can exist before login activation; participation in approvals requires an active linked account and applicable access.
- HR can invite staff and grant or revoke all business roles within HR's authorized organizational scope. Ownership and platform roles remain separately controlled.
- Preserve role scope, expiry, audit history, and the restriction against changing one's own access.
- Transfers, promotions, manager changes, and departures retain history. A departing manager's reports require reassignment; affected approval routes remain blocked until repaired.

### Organization chart

- Provide a searchable, expandable chart showing positions, staff, levels, reporting managers, branches, departments, and vacancies.
- HR can open a chart entry to add staff or edit placement, position, level, and manager.
- Clearly identify incomplete profiles and invalid reporting relationships.
- Use the same reporting data for the chart and approval routing.
- Staff visibility follows authorized organizational scope. Compensation, private HR records, and access-management details stay outside ordinary chart cards.

## 3. Organization messaging

Extend the existing Inbox and channels.

- Provide company, branch, and department channels linked to organization nodes.
- Membership follows active staff placement automatically: joining adds access, transfers update it, and departures remove it.
- Company membership includes that company's active staff; branch membership includes its staff across departments; department membership includes its assigned staff.
- Membership and attachment access must reflect current eligibility even when background synchronization is delayed.
- New members can read earlier channel history. Former members lose channel access, while their messages remain recorded.
- Existing direct messages, manually managed groups, and private channels retain their separate membership rules.
- Seniority, manager status, and HR administration do not automatically provide access to every department's conversations.
- Link chart entries to the appropriate authorized person conversation or organization channel.

## 4. Spending approval template

### Readiness

HR completes the organization and staff setup first, but readiness is checked **per request**. Unrelated incomplete staff profiles do not block the whole company.

A request can proceed only when its requester, reporting route, Accounting reviewers, and Finance payer satisfy the required profile, level, account, scope, and independence rules. The CEO exception removes only the organizational approval stage from the required route.

### Main flow

**Requester → nearest higher supervisor → Accounting → Finance payment with payment slip**

- **Requester:** All active staff may submit purchase requests, expense claims, and payment requests, including managers and Accounting/Finance staff.
- **Supervisor:** Require one approval from the nearest eligible, strictly more senior person in the requester's reporting chain. Do not select an unrelated senior colleague or require every higher manager to approve.
- **Accounting:** A preparer assembles the details. A different authorized Accounting reviewer at **Level 1–4** approves. This is a fixed supervisor-or-higher threshold, not a comparison with the previous approver's level. Preserve independent ledger-posting controls.
- **Purchases:** Accounting approves planned spending before ordering. Purchasing handles the order and receipt; Accounting validates the invoice and completes required posting before payment.
- **Finance:** An independent authorized Finance/Treasury person records the payment and attaches its bank payment slip. Every payment requires evidence; partial payments leave the outstanding balance open.
- Requesters and people preparing on their behalf cannot approve their own request. Preserve separation between organizational approval, Accounting preparation/approval/posting, and payment. Multiple roles held by one person do not make that person independent of themselves.
- Returns and rejections require reasons. Material corrections require renewed approval while retaining previous decisions.

### Templates and exceptions

- Use one company default, with explicit exceptions by organizational position or request type. Separate branch/department approval templates are outside this phase; reusable organization templates do not create them.
- **CEO:** Skip organizational approval only. Independent Accounting, purchasing where applicable, posting, payment, and evidence requirements remain.
- **Branch/department heads:** Route to their nearest eligible higher reporting supervisor.
- **Accounting/Finance requesters:** Use the normal route with independent participants.
- **Other missing-supervisor cases:** Block until corrected or covered by an explicitly configured exception. Do not inherit the CEO exemption or silently fall back to owner approval.
- Company-authorized administrators manage approval templates and record reasons for exceptions or reassignment. No other position receives an exemption by default.
- Submissions retain their template version, resolved route, and decision history. Later organization or template changes do not silently rewrite existing routes; unavailable or unauthorized reviewers require explicit repair.

## 5. Required interface extensions

These capabilities are delivered through the existing People, Inbox, expense, employee-request and finance interfaces, with the organization extensions below.

| Existing or new surface | Required behavior |
| --- | --- |
| People onboarding and profile management | Add position selection; supply and confirm its default level; select and validate the actual manager; retain incomplete onboarding drafts and explain activation blockers. Preserve person/account separation and staff-change history. |
| Invitations and role management | Allow scoped HR administrators to invite staff and grant/revoke business roles, while retaining scope, expiry, independent access changes, and separately controlled ownership/platform roles. |
| Organization settings | Manage positions, vacancies, default levels, expected reporting relationships, and reusable organization templates with review before applying them. |
| Organization chart | Query and display authorized organizational structure, occupants, actual managers, levels, vacancies, and setup issues; open the permitted staff editor or conversation from a chart entry. |
| Inbox, channels, and attachments | Associate organization channels with company/branch/department nodes and enforce current placement-based membership for channel discovery, messages, and files. Preserve manual and private conversation rules. |
| Approval administration and request detail | Configure the company spending template and explicit exceptions; report per-request readiness blockers and required repairs; display the resolved route, decisions, and template version. |
| Purchasing, Accounting, and Finance | Carry the approved request into purchasing and invoice validation, preserve separate posting controls, and record payment-specific evidence and the outstanding balance. |

## 6. Current implementation and compatibility

The implementation preserves legacy numbering and submitted approval evidence while applying the new policy to new spending submissions.

| Area | Delivered behavior and source |
| --- | --- |
| Levels, positions and chart | [Organization service](../../../src/server/organization.ts) installs six numbered levels, maintains positions and reviewed templates, validates reporting graphs and retains staff-change history. [Organization workspace](../../../src/components/modules/organization-workspace.tsx) shows staff, vacancies, placement, setup issues, editing and authorized conversation links. |
| HR onboarding and access | [People directory](../../../src/server/people-directory.ts) allows onboarding drafts and validates activation. [Account access](../../../src/server/account-access.ts) supports scoped HR business-role grants, revocations and invitations, with expiry, independent access changes and commit guards. |
| Organization messaging | [Organization channels](../../../src/server/organization-channels.ts) provides company, branch and department channels; [messaging](../../../src/server/messaging.ts) derives eligibility at read, send and download time. No copied membership list can preserve access after departure or transfer. |
| Spending policy | [Spending templates](../../../src/server/spending-templates.ts) stores a versioned company default and explicit position/request-type exceptions. [Expense routing](../../../src/server/expense-routing.ts) selects the nearest eligible higher reporting supervisor, requires independent Accounting preparation/approval and Finance, and preserves saved participants. |
| Purchase requests | [Employee requests](../../../src/server/employee-requests.ts) records supervisor, Accounting preparation and Accounting approval decisions before the existing Purchasing handoff, order, receipt and invoice workflow. Each decision has a reason and reviewed evidence version. |
| Payment requests | [Expense drafts](../../../src/server/expense-drafts.ts) supports a payment-request type, supporting documents and an optional existing supplier payee; employee payments use the requester as payee. The same approval, posting and payment controls apply. |
| Posting and settlement | Existing independent posting remains mandatory. Expense payments retain their bank slips; [supplier invoice settlements](../../../src/server/commerce.ts) now also require a unique attached slip for every payment, including partial settlements. |

### Organization interfaces

Under `/api/v1/holdings/:holding/people/organization`:

- `GET /` returns the scoped chart, positions, numbered levels, permitted templates and authorized channel links.
- `GET /readiness?kind=expense|purchase|payment` checks the current requester and their required independent team.
- `POST /levels` installs the numbered catalog for a new company.
- `POST /positions[/:id]` and `POST /templates[/:id]` save versioned positions and organization templates.
- `POST /templates/:id/apply` applies a reviewed structure as positions and vacancies.
- `POST /spending-template` saves a reasoned company policy revision. Position-and-type exceptions take precedence over position-only exceptions, then type-only exceptions. Accounting and payment controls cannot be exempted.

Mutations use existing authorization, origin checks, command idempotency, audit and optimistic-version controls. Staff activation and role grants remain separate actions. Repairs record a reason: expense repair replaces the eligible pending participant while keeping the submitted policy evidence; purchase repair retains previous evidence in request history and requires a renewed route. A changed manager or template does not silently replace submitted participants.

Migrations: holding-write `0010`–`0014` and control `0004`. Existing profiles remain explicitly unmapped, with their legacy levels retained. No production service or public Tunnel origin is changed by this delivery.

The [staff-work contract](../../contracts/next/staff-work.md) also describes current leave and task routing. The new spending template applies to purchase requests, expense claims, and payment requests; this task does not redefine leave, routine, or task approvals. Existing submitted records retain their route and decisions; later organizational changes that remove eligibility require explicit repair rather than silently substituting reviewers.

## 7. Acceptance scenarios

The automated acceptance coverage is in [organization](../../../tests/organization.test.ts), [organization channels](../../../tests/organization-channels.test.ts), [spending approvals](../../../tests/spending-approvals.test.ts), [expense workflow](../../../tests/expense-v2.test.ts), [employee services](../../../tests/employee-services.test.ts), and [workflow handoffs](../../../tests/flow-routing.test.ts). The full integration suite, type checking, lint and isolated release build provide regression verification. Browser verification uses the authenticated public development workspace; real staff data remains for HR to configure.

| ID | Scenario | Expected result |
| --- | --- | --- |
| HR-01 | HR creates a person, selects position and placement, confirms level, assigns a manager, activates the profile, invites an account, and grants business roles. | The person, engagement, account, scoped roles, and reporting relationship remain distinct and connected; the staff member is eligible for configured work. |
| HR-02 | HR saves incomplete onboarding information or attempts activation without a level or manager. | Draft remains editable; activation explains missing requirements. Only the explicitly designated CEO position is exempt from having a manager. |
| HR-03 | A manager selection references the same person, creates a reporting cycle, crosses companies, uses an inactive manager, or selects an equal/junior level. | Reject the invalid active reporting relationship and identify the correction needed. |
| HR-04 | A manager is Level 2 and the selected staff position defaults to Level 6. | HR can confirm Level 6; the system does not force Level 3. |
| HR-05 | HR changes a position occupant, fills a vacancy, promotes or transfers staff, or records a manager's departure. | Position identity and historical changes are retained; actual staff level, placement, and manager are validated; affected routes identify any required reassignment. |
| HR-06 | HR grants/revokes roles outside their scope, changes their own access, or attempts to grant ownership/platform authority. | Deny the unauthorized change; authorized business-role changes preserve expiry and audit evidence. |
| ORG-01 | HR applies an organization template and reviews its positions, levels, and reporting structure. | Changes require review; vacancies and shared position types are supported; no role grant is generated automatically. |
| ORG-02 | Staff search and expand the chart; HR opens a chart entry to edit a manager. | The chart shows authorized data and actual reporting relationships matching approval routing; ordinary cards omit compensation and private HR/access details. |
| CHAT-01 | Staff join a company and receive branch/department placement. | They gain the matching organization channels and can read earlier messages; an authorized chart link opens the correct conversation. |
| CHAT-02 | Staff transfer or depart while channel-membership synchronization is delayed. | Channel discovery, message access, and attachment downloads use current eligibility; old messages remain recorded without preserving former-member access. |
| CHAT-03 | A senior manager or HR administrator opens an unrelated department channel; staff placement changes while they belong to a private channel. | Seniority/HR status alone grants no unrelated-channel access; private/manual channels keep their own membership rules. |
| APPROVAL-01 | Level 6 staff have an eligible Level 4 direct manager and higher managers above them. | Require the Level 4 manager's single organizational approval, followed by Accounting and Finance. |
| APPROVAL-02 | A branch/department head requests spending, or the designated CEO requests spending. | The head routes to an eligible higher reporting supervisor; the CEO skips only organizational approval. |
| APPROVAL-03 | Accounting or Finance staff request spending, including a person who holds several roles. | Use independent participants; requester/preparer self-approval and conflicting payment participation remain denied. |
| APPROVAL-04 | Authorized Accounting candidates have Level 4 or Level 5, and the organizational approver has Level 2. | Level 4 satisfies the Accounting threshold despite being less senior than the organizational approver; Level 5 does not. |
| APPROVAL-05 | A purchase moves from request through ordering, receipt, invoice, posting, and payment. | Accounting approves planned spending before ordering; subsequent invoice validation and required posting occur before independent Finance payment. |
| APPROVAL-06 | Finance attempts a payment without a slip, then records two partial payments with their respective slips. | Missing evidence blocks confirmation; each recorded payment has its own evidence and the balance remains open until fully settled. |
| APPROVAL-07 | A request has missing organizational data or an unavailable/unauthorized participant, while unrelated staff also have incomplete profiles. | Block and explain this request's missing requirements; unrelated incomplete staff do not block a ready request. Non-CEO missing-supervisor cases do not receive an automatic exemption. |
| APPROVAL-08 | A request is returned/rejected, materially corrected, or affected by later manager/template changes. | Reasons and earlier decisions remain visible; material changes require renewed approval; saved routes are not silently rewritten and ineligible reviewers require explicit repair. |

## 8. Defaults and delivery boundaries

- Application delivery order is organization/position setup, HR onboarding and role management, chart and organization-channel integration, then request-readiness and spending-template integration. Readiness remains per request rather than a company-wide lock.
- The six levels above are the initial catalog. Explicitly map existing staff and levels; access roles are never a substitute for a seniority decision.
- “Payment slip” means evidence of an external bank payment. “Payslip” means an employee payroll statement.
- Payments occur outside ERPX and are recorded with bank payment evidence. Bank-transfer integration is future work.
- Position-based automatic role grants, separate branch/department approval templates, and changes to unrelated approval flows are outside this phase.
- The original documentation task was followed by the user-authorized application implementation. HR must review and map real staff, managers and positions before using the new spending policy. Existing leave, routine, task and manual/private-channel rules are retained.

## 9. Delivery verification — 2026-09-14

- Full integration suite: **254 tests passed across 32 files**.
- `bun run typecheck`, `bun run lint`, `bun run deploy:build`, and `git diff --check`: passed.
- Development migrations applied to the configured development databases; the existing detached Node 24.20.0 development server remains on port 3100.
- Local and public `/api/health`: HTTP 200. Authenticated public-browser checks confirmed the Organization chart, spending-template controls, readiness feedback, Inbox and Home load.
- The current public account has no active linked staff engagement, and the chart reports 20 staff profiles needing setup. HR must explicitly configure real positions, levels, managers and account links. Production port 3101 and the existing public Tunnel origin were preserved.
