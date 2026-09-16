# ERPX enterprise UX/UI delivery — 2026-09-06

The live Next.js workplace now shares one responsive design system across login, Home, Work, Updates, People, Operations, Finance, Projects, Learn, Inbox, account controls, and request review. The former development module catalog is not a supported workspace destination. Delivery is local; deployment, transactional backend expansion, migrations, and dependency changes are outside this redesign.

## Review locally

Use the existing development server at `http://localhost:3100`. Start it with `bun run dev` only when that port is free. Sign in with an existing development account, then visit `/holdings/h001/home` or an authorized People, Operations, Finance, or Projects destination.

Light is the default. Appearance is saved in a cookie and rendered by the root layout; System follows the OS through CSS. Desktop and tablet navigation use authorization-aware parent/child groups; mobile keeps Home / Work / Inbox / More and opens the same hierarchy in a bottom sheet. Search is available from the header and Cmd/Ctrl+K. Mobile request actions take the place of bottom navigation while a request can be acted on.

## Coverage and availability

The retained module registry supplies permission metadata and backend compatibility only. Workspace navigation exposes canonical live destinations grouped under People, Operations, Finance, Projects, Resources, and Administration. Removed module, section, and preview-record URLs resolve to the standard unavailable view without redirects.

All nine workspace layouts are implemented: operational tables, boards, ledgers, analytics, timelines, libraries, directories, service status, and conversations. Sample fields and measures are tailored to capability context. Details, forms, reviews, and related record links remain inspectable.

Capability and action availability is defined in `src/data/module-registry.ts`. Existing leave submission, independent approval, evidence uploads/downloads, updates, comments, reactions, follows, and Stories remain live through their existing business APIs. Device accounts and assignment switching retain their existing contracts. A capability with some supported actions is labeled as partial; additional workflows and moderation remain previews.

Unsupported screens carry **Preview · Sample data**. Their forms and business actions are disabled with an explanation. All sample record URLs use `/holdings/[holdingSlug]/preview/records/[module]/[id]`; no preview business action sends a mutation request. Preview datasets live in a separate server-only adapter, and Home/Work only query authorized live records. The command palette indexes navigation metadata, never sample business records. Platform operations are fictional. Learn reuses the original three captioned videos; Inbox is explicitly a notification preview.

Each Holding module section also exposes seeded **Live workflow** records when the selected assignment contains that exact module ID. The shared workbench supports optimistic field edits, explicit Draft → In review → Approved/Rejected → Completed transitions, independent review, typed role-labelled `@module/record` edges, typed `@user/name` mentions, backlinks, and evidence-pinned field suggestions that require Apply or Reject. The development suggestion provider is deterministic and labelled as such; it proves the human-approval, evidence-version, and conflict UX without implying that an external model is configured.

The same typed `@` picker is active in the Updates composer and comment editor. Selected people and records are submitted as structured targets, validated again on the server, and persisted as mention edges alongside the idempotent post or comment command. Live records support create, read, update, workflow transitions, and recoverable archive; create forms render every documented field for their capability section.

Home calculates its totals before limiting visible priorities to four. Work uses URL-based queues and search, including restoration from record views and browser Back. Request details retain handover notes, uploaded evidence, and decision notes after completion. Review dialogs contain focus, support Escape, and restore focus. Failed leave commands preserve the review and reuse the same idempotency key on retry.

## Files and extension points

- `src/app/globals.css`: global tokens, resets, typography, controls, reduced motion, and both appearances.
- `src/components/ui/`: shared headings, metrics, badges, empty/loading states, and accessible dialogs.
- `src/components/workplace/`: shell, command search, appearance, account/login/social/page styles.
- `src/components/modules/`: directory filters, module navigation, and the nine reusable workspaces.
- `src/data/preview/`: separate prototype fixtures and capability-specific fields.
- `src/server/work.ts`: full authorized work summaries and evidence reads.
- `tests/ui-registry.test.ts` and `tests/e2e/`: coverage, isolation, navigation, account, and leave regression checks.

The old `enterprise.css` and `social.css` files are no longer imported. Their pre-existing source is preserved, along with the original prototype and other uncommitted work. Active styling uses global foundations and scoped component styles.

To refresh registry data from the original prototype, run `bun scripts/generate-ui-registry.mjs`. This reads only its data declarations and capability manifest and writes the two generated JSON files. It does not change or execute the prototype's browser command layer.

## Verification

Required commands: `bun run lint`, `bun run build`, `bun run build:cloudflare`, `bun run test:integration`, and `ERPX_E2E_BASE_URL=http://localhost:3100 bunx playwright test`.

Final verification on 2026-09-06 passed: lint, Next.js production build, Cloudflare/OpenNext build, all 8 integration tests, and all 18 applicable browser tests. The two skipped mobile cases duplicate route coverage and the explicit four-width matrix run in the desktop project. The matrix produced 72 screenshots with no page-level horizontal overflow. All nine layouts were visually inspected in light and dark appearance, along with live sign-in, Home, Updates, and mobile navigation.

The browser suite checks all 31 module entries, 171 capability routes and their sample forms, original preview record details/forms/reviews, invalid routes, URL filters, browser Back, zero preview mutation requests, caption playback, saved appearance, reduced motion, keyboard focus, long drafts, and recoverable failures. The layout matrix covers all nine layouts at 1440, 1024, 768, and 390px in both light and dark appearance and saves screenshots under `test-results`.

The local leave journey creates uniquely named temporary fixtures, verifies six actionable requests produce a full Home total while only four are displayed, uploads evidence, simulates a dropped request, retries, and approves from a separate account. Its cleanup removes only its own records and files. Local fixture creation is refused for remote E2E hosts; no production data is changed.
