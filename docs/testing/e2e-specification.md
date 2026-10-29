# ERPX end-to-end specification and test cases

This document defines the foundation authentication/Work browser cases first reviewed on 2026-09-12. Full-catalogue engineering cases are mapped in [the current scenario register](../../tests/catalogue/engineering-scenarios.json); [the verification inventory](../status/capability-delivery-matrix.md) records the dated result separately. This specification does not certify every source revision.

## Scope and sources

The current application combines daily work in `/holdings/{holding}/work`.
Requests and staff-task list routes redirect there. Creation options depend on
the authenticated member; document services own state and enforce authorization.
The Work page preserves queue, type, responsibility and search in the URL, and
refreshes its summary after focus/restoration. Request forms retain unsaved fields
in session storage and reuse an idempotency key for an unchanged failed command.

Read alongside these implementation sources:

- [Work page](../../packages/feature-apps/src/routes/holdings/[holdingSlug]/work/page.tsx),
  [dashboard](../../packages/workplace/src/components/work-dashboard.tsx),
  [results](../../packages/workplace/src/components/work-list.tsx), and
  [new-work dialog](../../packages/workplace/src/components/new-work.tsx).
- [Request editor and actions](../../packages/workplace/src/components/request-controls.tsx),
  [request service](../../packages/server-records/src/public/employee-requests.ts),
  [browser draft recovery](../../packages/feature-shared/src/components/use-recoverable-draft.tsx), and
  [command retry](../../packages/feature-work_projection/src/components/staff-work-controls.tsx).
- [Named workflow handoffs](../production-plan.md),
  [local personas and connected examples](../local-seed.md), and
  [module inventory](../production-plan.md).

The older module inventory contains historical Cloudflare runtime evidence.
Current acceptance uses Node/PostgreSQL. In particular, accounting approval and
GL posting are separate steps in the newer handoff implementation; an older
journey that treats them as one step is not the current acceptance contract.

## Execution and evidence

Use Node 24.20.0, Bun 1.2.17 and installed Playwright Chromium. Reuse the persistent
dev server on 3100. Tests must not start a release build in its output directory.

```bash
bun run test:e2e:smoke
bun run test:e2e:work
bun run test:e2e:work --project=desktop --grep REQ-001
bun run test:e2e:report
```

Both desktop Chromium and Pixel 7 Chromium emulation run each new case. Mobile
emulation is not iOS/WebKit or physical-device certification. Every test gets a
fresh browser context. Work journeys authenticate the documented seeded Mali
account through the real sign-in API, then exercise the browser UI. They require
the local development database; a remote base URL skips this file explicitly.
Missing local accounts are failures, not silent skips. UI login itself is tested
in `auth-smoke.spec.ts`.

REQ-001 creates one uniquely named service-request draft per run/project, cancels
it through the UI, and retains its audit history. Its `finally` block attempts to
cancel a remaining saved draft after failure. It never submits to a service team.
Failure injection intercepts only the individual browser's targeted request;
successful retries and persisted-state assertions use the real server/database.
The save failure occurs before the server commits; this does not prove recovery
from a lost response after commit. That stronger replay case remains FLOW-003.

Use test IDs in Playwright titles to select cases. A case passes only when its
assertions pass; a fixture skip is not passing acceptance. Failure screenshots and
traces are in ignored `test-results/`; the HTML report is in `playwright-report/`.
Document each run's command, date, project, pass/fail/skip count, and blocker.

## New executable Work cases

All cases below map to [work-journeys.spec.ts](../../tests/journeys/shared.work/work-journeys.spec.ts).

| ID / priority | Preconditions | Steps | Expected result |
| --- | --- | --- | --- |
| WORK-001 / P1 | Seeded Mali, Work accessible | Select Requested by me and Service request; search a unique nonexistent title; switch to Waiting; reload; clear filters | Empty result is explicit; all selected values survive navigation/reload; Waiting is current; clearing resets search/type/responsibility while preserving queue |
| WORK-002 / P1 | Seeded Mali | Open `/holdings/acme/requests` | Redirects to Work with `type=service_request`; matching type and Your work heading render |
| WORK-003 / P1 | Seeded Mali | Open `/holdings/acme/staff-tasks?queue=completed&q=acceptance` | Redirects to Work with `type=staff_task`, Completed queue and unchanged search |
| WORK-004 / P1 | Seeded Mali | Open New; inspect Service request; press Escape | New work modal closes and focus returns to New |
| WORK-005 / P1 | Seeded Mali; browser-only 503 on summary refresh | Open filtered Waiting list; observe failure; remove interception; click Retry work | Error is visible while results/filter UI remains; real refresh returns 200; retry/error clears; search/type remain |
| REQ-001 / P0 | Seeded Mali; new unique title; browser-only failure on first create | Open Service request from filtered Work; enter title/details; reload; fail first Save draft; retry; inspect persisted draft; return to Work; open the single matching row; cancel; reload/read state | Fields survive reload/failure; unchanged retry uses the same nonempty idempotency key; API reports draft; exactly one matching result exists; return search survives; cancellation persists and editing is removed |

## Authentication baseline

[auth-smoke.spec.ts](../../tests/e2e/identity_access/auth-smoke.spec.ts) implements these cases;
the IDs here are documentation references, not existing title prefixes.

| ID | Steps | Expected result |
| --- | --- | --- |
| AUTH-001 | GET health | 200, `{status: "ok"}`, no-store |
| AUTH-002 | In fresh contexts open root, holding Home and holding Work | Each redirects to Login with visible credentials fields |
| AUTH-003 | Submit nonexistent account credentials | Real auth returns 401, error appears, Sign in is enabled again |
| AUTH-004 | Follow Forgot password, Verify email and Join the free beta separately | Correct URL and page heading; no email or signup submission |
| AUTH-005 | Sign in using valid holding member credentials; reload Home | Holding Home and authenticated main remain; no Sign in button |

## Existing workflow coverage to retain

These are concrete acceptance cases mapped to existing specs, not results of the
current run. Inspect fixture configuration before running them. Several cases
span multiple assertions in one existing Playwright test.

| ID / priority | Setup and steps | Required outcome | Existing spec |
| --- | --- | --- | --- |
| STAFF-001 / P0 | Mali creates project/task; Ava approves start; Nisa executes and uploads evidence; Mali approves completion | Persisted completed task; each actor sees only their allowed action; unrelated user cannot download evidence or read task | `staff-work.spec.ts` |
| STAFF-002 / P1 | Create routine; save paused monthly day-31 schedule; resume; pause; create occurrence | Correct schedule controls and independent task occurrence | `staff-work.spec.ts` |
| LEAVE-001 / P0 | Employee submits leave; independent reviewer decides; revisit from another session | Persisted decision is shared, self-review is not granted | `leave-approval.spec.ts` |
| IAM-001 / P0 | Owner edits level, issues activation and changes role grants | Distinct activation/access commands and role boundaries remain | `administration.spec.ts` |
| IAM-002 / P0 | Authorized editor reroutes approval; second editor attempts stale revision | One authorized change; stale proposal cannot overwrite current route | `administration.spec.ts` |
| EXP-001 / P0 | Member without completed profile saves incomplete expense | Draft retained with actionable People setup link | `expense-v2.spec.ts` |
| EXP-002 / P0 | Independent people perform business/accounting review, explicit GL posting and two slip-backed payments | GL/payable exist only after posting; payments persist with evidence and correct remaining balance | `expense-v2.spec.ts` |
| EXP-003 / P1 | IT saves provider draft; employee attempts equivalent access | Secret remains masked; unauthorized settings access denied | `expense-v2.spec.ts` |
| EXP-004 / P1 | Generate AI field suggestions around manual edits; request replacement | Empty fields may fill; manual changes require explicit replacement | `expense-v2.spec.ts` |
| REQ-002 / P0 | Service team takes request, asks question, receives evidence, resolves; requester reopens then confirms | Shared progress and final requester confirmation persist | `shared-workspace.spec.ts` |
| REQ-003 / P0 | Equipment request receives independent approval and handover | Handover evidence recorded; no invented accounting entry | `shared-workspace.spec.ts` |
| REQ-004 / P0 | Purchase request becomes approved order and receipt; repeat handoff | One linked purchase handoff, correct permission checks | `shared-workspace.spec.ts` |
| KNOW-001 / P1 | Editor publishes handbook revision and starts next draft; employee browses/searches | Published revision is visible; next draft remains private | `shared-workspace.spec.ts` |
| CARD-001 / P1 | Save pin order on device A; refresh B; submit stale arrangement | Preferences sync; conflict preserves proposed arrangement | `work-cards.spec.ts` |
| CARD-002 / P0 | Try forged preference authority and vary company/branch/department | No authority escalation or cross-scope card data | `work-cards.spec.ts` |
| CHAT-001 / P0 | Independent members chat, reconnect, attach files and use teams; outsider reads | Messages recover, private files and membership are enforced | `messaging.spec.ts` |
| SOCIAL-001 / P1 | Post → manual task → evidence → independent acceptance with AI fallback | Conversation links to persisted work and accepted result | `social-beta.spec.ts` |
| NAV-001 / P1 | Browse the authorized parent/child sidebar, collapse groups, trigger short-height overflow, open the mobile bottom sheet and use command search | Canonical destinations appear once, active ancestors reopen, restricted pages remain hidden and controls are keyboard accessible | `enterprise-ui.spec.ts` |

Fixture dependencies:

| Spec | Required setup / limitation |
| --- | --- |
| `staff-work`, `leave-approval`, `administration`, `social-beta`, `enterprise-ui` | Documented seeded personas; some tests create persistent acceptance records or skip mobile duplicates |
| `expense-v2` | `.scratch/expense-v2/fixture.json` via `scripts/seed/seed-expense-e2e.ts`; explicitly set the local `ERPX_E2E_BASE_URL` because helper cookie defaults still use 8787 |
| `shared-workspace` | `.scratch/shared-workspace/fixture.json` or `ERPX_SERVICES_FIXTURE`, via `scripts/seed/seed-services-e2e.ts`; set explicit local base URL |
| `work-cards` | `.scratch/work-cards/fixture.json` via `scripts/seed/seed-work-cards-e2e.ts`; set explicit local base URL because helper default differs from runner |
| `messaging` | `.scratch/messaging/fixture.json` via `scripts/seed/seed-messaging-e2e.ts`; explicit local base URL required |
| `local-review` | Imports Wrangler `getPlatformProxy`; audit/migrate this historical fixture access before native runtime acceptance |

Do not execute all fixture scripts blindly: review their target guards and the
development environment first. Existing AI-provider scenarios have documented
historical failures in the handoff notes; they remain separate from expense
accounting acceptance.

## Additional browser cases still to implement

These cases follow current domain boundaries but are not implemented by the new
Work spec. Existing API/integration coverage cannot substitute for these browser
journeys. Use separate fixture SKUs/documents and distinct authorized people.

| ID / priority | Preconditions and steps | Expected result |
| --- | --- | --- |
| FLOW-001 / P0 | Ambiguous team candidates; start document; admin chooses eligible primary with reason | Route blocks until assigned; responsible person and audit reason persist |
| FLOW-002 / P0 | Start document from default revision A; change default to B; open old/new documents | Old snapshot stays A; new document uses B; explicit stale reassignment rejected |
| FLOW-003 / P0 | Proxy a real committed command but lose its response; retry identical command | Same idempotency receipt/result; no duplicate record, posting, allocation or notification |
| FIN-001 / P0 | Daniel prepares balanced journal; independent Nisa posts; replay; reverse eligible journal | One posting, balanced debit/credit; original retained alongside reversal |
| FIN-002 / P0 | Create/post invoice; Treasury records partial then remaining settlement | Posting precedes settlement; references/allocations retained; outstanding reaches zero; overpayment denied |
| STOCK-001 / P0 | Operator counts isolated SKU; independent reviewer approves; inspect stock; explicitly post | Approval alone changes no inventory/GL; posting atomically changes quantity/value and balanced journal |
| STOCK-002 / P0 | Prepare count then move stock before posting; attempt stale post | Clear stale-snapshot rejection, no partial stock/GL effects; recount can proceed |
| STOCK-003 / P0 | Source operator prepares transfer; independent eligible receiver confirms | Source/target paired movements preserve combined quantity/value; self/wrong-branch review denied |
| ORDER-001 / P0 | Purchase order approval → partial receipts → accounting journal posting → invoice posting → Treasury | Correct links and separate actors/actions at each stage; excess receipt and repeated conversion rejected |
| ORDER-002 / P0 | Quote approval → sales order → partial delivery → journal posting → invoice → partial receipts | Stock/COGS and AR/tax linked correctly; remaining fulfillment/payment stays actionable |
| PAY-001 / P0 | Approved gross-pay preview; attempt production posting/payment | UI identifies missing statutory adapter; no production liability or payment created |
| SCOPE-001 / P0 | Open private document/file with authorized then foreign-company/revoked session | Foreign/revoked requests denied server-side; no protected content retained after refresh |

## Acceptance boundary

Passing the new suites certifies the listed authentication and Work/request cases
only. The retired development module catalogs are explicitly checked as 404s;
the suites do not certify every registry capability, all currencies, payroll production,
payment execution, email delivery, external AI availability, or every workflow
handoff. Promote an additional case to verified only after executing its browser
assertions against the current native runtime and recording the actual result.

## Execution record — 2026-09-12

| Command / check | Actual result |
| --- | --- |
| `bun run test:e2e:work` | WORK-001–005 passed on desktop and mobile (10 passes). REQ-001 failed on both because the exact label selector did not find the restored textarea; the failure snapshot showed the correct preserved value. |
| `bun run test:e2e:work --grep REQ-001`, after switching to accessible textbox role/name | 2 passed in 37.9s. Real saved draft, unchanged retry key, single result and persisted cancellation verified on desktop/mobile. |
| Public `auth-smoke.spec.ts`, selecting signed-out `/holdings/acme/work` | 2 passed in 3.6s; both browser projects reached the login page. This was not a public authenticated business journey. |
| `bun run typecheck`, `bun run lint`, `git diff --check` | Passed. |
| Local and public `/api/health` | Both HTTP 200; existing workspace dev listener on 3100 retained. |

All six new logical Work/request cases have passing desktop and mobile evidence
across the runs above. Existing business suites in the inventory were not run as
part of this specification update. The HTML report reflects the latest local
targeted run, not an aggregate of all commands.
