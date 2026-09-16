# Workplace design acceptance — 2026-09-13

Implemented the shared stone/charcoal and cobalt palette, person avatars, compact Work shortcuts and workflow rows, shared Home/Work attention summary, responsive Home feed, and Inbox conversation and Bangkok-day notification styling. Existing document routes, approval rules, public APIs and database schemas are unchanged by this UI work.

## Verification

- Typecheck and lint passed with Bun 1.2.17 and Node 24.20.0.
- Isolated release build passed: `.runtime/releases/2026-09-12T17-24-13-049Z`. No production activation was performed.
- Unit tests: 30 passed across attention, Work Cards, preferences, workspace and navigation suites.
- Browser suites: 42 passed and 3 intentionally skipped across workplace design, Work Cards, Work journeys, Social, Messaging and selected enterprise navigation cases.
- Responsive coverage: Home, Work and Inbox at 1440×900, 1024×768 and 390×844 in Light/Dark; System color schemes and reduced motion also passed.
- Coverage includes semantic contrast, overflow, touch targets, keyboard dialog dismissal/focus, Thai and long names, current/next Flow responsibility, filter preservation, summary totals beyond three items, refresh failures and revoked access.
- Work Cards ran against its isolated fixture database and temporary server on 3102. Existing migrations were applied to that test database to match the current branch. The temporary server was stopped after verification.
- The canonical navigation case now uses the seeded Bangkok Finance department path. Application routing and permissions were not changed for that test.

## Public acceptance

Inspected Home, Work and Inbox through the existing authenticated session on [ERPX](https://erpx.jesadakorn.com). Desktop Work exposes queues, filters and the first result within the first viewport; Home displays the first post alongside attention, with attention above the composer on mobile. Light and Dark appearance were inspected. The user's original Dark preference and normal viewport were restored.

Final local and public `/api/health` checks both returned HTTP 200. The existing development listener remains on port 3100; production on 3101 and the Tunnel configuration were preserved.

The two feed posts created by the Social regression run were hidden through the existing moderation action after testing; their linked tasks and audit history were retained.
