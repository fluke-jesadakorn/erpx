# UX and Retirement implementation verification — 2026-09-17

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


## Delivered

1. Shared daily navigation and searchable All apps; up to six persistent pins with stable defaults, versioning, Company/workspace isolation and current permission checks. Work presets, due ordering, separate setup/team work, compact existing shortcuts and incomplete-source indicators.
2. Status/owner/next-action summaries and prominent domain controls on the nine requested record families; preserved Work return queries. Intent-based New work, recoverable draft-before-upload requests, retained optional expense AI, shared Home priorities and Messages/Teams/Notifications in Inbox.
3. Retirement as an offboarding reason, explicit timezone/cutoff, six clearance responsibilities, owner permission enforcement, live source evidence, separate effectiveness/access/directory/clearance signals, People filters and six repeatable fictional development examples.
4. Calendar presets/filter chips/collapsed hierarchy, event versus coverage people, browser history and roster department/week/date handoff. Finance queues and source links; shared readiness checklists for People, Approvals, POS and PMS.
5. Shared list filters and empty states for Projects, Routines, Handbook and Learn; existing Billing/Administration headers, dialogs and specialist controls retained. Responsive containment fixes for Calendar and financial tables; nested dialogs close only the top dialog and restore focus.

Contracts: [Workplace UX](../../contracts/next/workplace-ux.md), [People lifecycle and seed guide](../../contracts/next/people-lifecycle.md), [Calendar](../../contracts/next/calendar.md). Terminology is recorded in [CONTEXT.md](../../../CONTEXT.md).

## Automated verification

- Pinned Bun 1.2.17 and Node 24.20.0.
- Additive migrations 0039 and 0040 applied to the development database. Retirement seed run twice: six creations followed by six unchanged skips; no provider actions.
- Full Vitest suite: **575 passed, 2 opt-in provider tests skipped**, 60 files passed and 2 skipped. The first full run exposed only the expected baseline table count change (231 → 232); the corrected migration test and full suite passed.
- Coverage includes employee/manager/HR/IT/Finance/Owner domain permissions, Company and local scopes, independent approvals, partial receiving and payments, unavailable work sources, current authority/version checks, request upload recovery, payroll/financial evidence redaction, all-account cutoff independent of workers, cancellation, rehire, post-cutoff clearance and repeat seeding. Existing POS/PMS/roster tests cover their original retry and handoff rules.
- Typecheck, ESLint and isolated `deploy:build` passed. Builds used `.runtime/build` and `.runtime/releases`; the active dev output was not used by the release build.
- `git diff --check` passed. Preexisting tracked and untracked work was preserved; no commit or production deployment was performed.

Run logs are local ignored artifacts under `.scratch/ux-rollout/`: `tests-final.log`, `typecheck-final.log`, `lint-final.log` and `release-build-final.log`.

## Authenticated public browser verification

Used the existing normal signed-in Owner session at `https://erpx.jesadakorn.com`; no authentication bypass or real financial/directory/retirement submission was used.

- Inspected representative layouts at **390, 768 and 1440px**, in light and dark appearances. Work, All apps, Calendar, People, clearance, request creation, invoice detail, Finance, POS and PMS were inspected. Document-level overflow found in the Calendar week grid and invoice list was fixed so their tables scroll internally.
- All apps search works; Escape returns focus to its opener. Nested People/employment dialogs now close one at a time and restore focus to Continue draft. Native date editing updates cutoff to next-day midnight without saving the demo case.
- Calendar Team mode showed 13 event rows and Coverage 119 rows including people without shifts. Back restored Team filters. Embedded roster opened the same week. Organization nodes start collapsed.
- Upcoming retirement and Retired each show two relevant seeded examples. Back restores the selected filter. The effective demo with pending finance/payroll remains visible and shows separate access/directory/clearance signals.
- Request creation shows attachments, editable details and review together; retry/reload failure behavior is covered by automated tests, not a forced upload failure against the public session.
- Finance To collect opened the filtered original invoice list; invoice detail showed partial-payment status, responsible team, blocker and the original accounting links. No receipt/payment/posting action was performed.
- POS correctly showed incomplete accounting/open-session setup, with links to resolve it. The current development Company has no PMS hotel, so browser verification covered the first-hotel setup state. Booking → stay → folio and retries were verified in integration tests, not a live hotel browser journey.
- Inbox displayed Messages, Teams and Notifications. Home retained team news/conversations and automatically loads up to three priorities from Work after hydration, without blocking the feed.

These are representative visual checks and automated domain-role checks; no claim is made that every role was manually switched through every page at every viewport.

## Availability

The existing ERPX development server was reused during implementation, then restarted on **3100** with a fresh development deployment ID for final public asset verification. The replacement `bun run dev` supervisor is detached and survives task completion. Its process working directory is this repository. Local `/api/health` and public `https://erpx.jesadakorn.com/api/health` returned **HTTP 200**. Production **3101**, other projects' ports and the Tunnel configuration/origin were not changed. Browser viewport overrides were cleared and the original dark appearance restored after verification.
