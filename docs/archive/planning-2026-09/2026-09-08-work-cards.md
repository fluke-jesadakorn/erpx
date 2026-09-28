# Work Cards

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


Work now opens with **Your shortcuts** above the existing queues and results. Cards use ERPX colors, typography and controls. Home remains the team feed, and Administration remains under More.

## Using the cards

- Select a card's main area to filter the list below. Type cards select `type` and `queue=all`; Approvals selects `responsibility=approval&queue=todo`; Team queue selects `responsibility=team&queue=all`. Search and unrelated filters are cleared.
- **New** inside a card opens the existing form and preserves the current return filters. The page's **New** menu uses the same server capabilities. Beta Tasks offers **Start from a conversation**.
- **Customize** selects up to six pins, including any saved pins temporarily hidden in another scope. Up/down buttons change order; Save commits it, Cancel discards changes, and Reset to default takes effect when saved.
- An explicitly empty selection stays empty. **All actions** exposes all available cards without changing saved pins. Defaults prioritize pending approvals and team work, then the fixed daily-work order.
- Mobile initially shows three pins and an expansion button. Layouts use one, two and three columns. Reordering and removing a pin retains keyboard focus within the editor; closing it returns focus to Customize.

## Data and authorization

`lib/work-cards.ts` owns stable IDs and labels. `server/work-capabilities.ts` supplies the creation menu, direct creation URL checks and card creation links. It uses applicable assignments in the current company and scope. A type card requires creation authority or accessible work; Approvals requires an actionable decision, and Team queue requires current team membership with active overlapping coverage. Pins never grant command authority.

`server/work-summary.ts` combines the existing Work snapshot, capabilities and preferences. Cards count the same deduplicated records used by the list. One failed domain marks affected card counts incomplete instead of zero while other work remains usable. The client refreshes the combined summary on return, tab focus, visibility changes and persisted browser restoration, without individual card requests. Creation and decisions continue through their existing record pages and authorization.

`GET /api/v1/holdings/:holding/work/summary` retains existing items and next steps and adds cards, capabilities, preferences and load status. Scoped routes use the existing workspace proxy. `GET/POST .../work/card-preferences` operate only on the session user and company; request bodies cannot choose identities or supply URLs.

The `work_card_preferences` table stores company/user, nullable ordered ID JSON, version and update time. Null means defaults; an empty array means no pins. Saves require an idempotency key, expected version, unique registered IDs and at most six pins. New pins must be available now. Existing pins from other scopes can be retained. Membership, assignment and service authority are checked again inside the command transaction; writes, receipt, audit and outbox remain atomic.

A version conflict preserves the proposed order, loads the latest saved arrangement and requires review before another save. Failed preference reads show defaults and Retry; customization remains disabled until the saved version can be read. Network failures preserve the current view with a stale-data notice. Revoked workspace access clears actionable cards on refresh.

## Runtime and migration

The additive legacy holding migration is `0042_work_card_preferences.sql`. During this implementation, the separately authorized local-runtime migration replaced Cloudflare execution with Node and PostgreSQL. Its native holding baseline includes the same preference table. Use the active runtime provisioner/migration runner before serving these APIs; do not run a UI rollback by dropping saved preferences.

The active verification gates are Next production build, PostgreSQL integration and native browser checks. The runtime migration replaces the previous Cloudflare build gate. Work Cards does not change business workflows or deploy the application.

## Verification

Tests are in `tests/work-cards.test.ts`, `tests/work-card-preferences.test.ts` and `tests/e2e/work-cards.spec.ts`. They cover card/count filtering, overlapping roles, profile-free drafts, beta, team/department coverage, inaccessible and hidden pins, separate users/companies, concurrent decisions, idempotency, transactional revocation, API rejection, cross-device conflicts, source failures, refresh/Back and keyboard behavior.

The browser fixture script `scripts/seed-work-cards-e2e.ts` is restricted to `.runtime/work-cards-e2e`, an isolated local database manifest. It creates synthetic ERP and beta companies and writes private test account details to `.scratch/work-cards/fixture.json`. It must not be pointed at user or production databases.

Local evidence is under `.scratch/work-cards/`: `build.log`, `lint.log`, `integration.log`, `e2e.log`, and `screenshots/`. Screenshots cover 390, 768 and 1440px in light and dark themes, including customization. The test asserts no horizontal overflow, touch controls of at least 44px and modal focus containment/return.

Verified on 2026-09-08:

| Gate | Result |
| --- | --- |
| ESLint | Zero errors; two warnings in the concurrent runtime migration's fixture/seed scripts |
| Next production build and TypeScript | Passed; one runtime credential-file tracing warning remains owned by the runtime migration |
| PostgreSQL migration/integration suite, after build | 20 files, 145 tests passed, including 15 focused card/preference tests |
| Work Cards browser journeys | 13 passed; one duplicate mobile matrix case intentionally skipped |
| Native standalone smoke | Authenticated Work HTML, summary API, preference API and client asset passed |
| Visual/keyboard matrix | 390/768/1440px, light/dark, 44px controls, no horizontal overflow, dialog focus and keyboard reorder passed |

The isolated build directory is under `.scratch`, which the runtime migration excludes from standalone tracing. The smoke bundle therefore copies its compiled server chunks and static assets into the standalone directory before running. The production release's normal `.next` packaging and deployment remain the responsibility of the separate runtime migration. The isolated preview on 3102 was stopped after verification.
