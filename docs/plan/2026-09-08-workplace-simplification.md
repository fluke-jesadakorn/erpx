# Workplace simplification and Administration

Implementation of the approved UX/UI plan, 2026-09-08. Home remains an English team feed. No database migration, role consolidation, deployment or production data changes are part of this delivery.

## 1. Navigation and Administration

- Holding navigation is Home, Work, Inbox and More. Sidebar, mobile, search and active destination use `workplace-navigation.ts`. Platform has its own supported destinations.
- Workspace contains company, scope and authorized role selection. Accounts and appearance remain in the account menu.
- Settings is Administration, with individually authorized People & access, Organization, Approvals & routines, Usage & billing and Feedback & reports sections.
- Ordinary ERP members retain access to published routines through the Approvals & routines section, because that capability was already available. They receive no approval queue, record details or rerouting controls. Their default Settings destination remains feedback. Beta members retain feedback only; beta owners see supported company tools.
- Legacy feed/post links, staff-settings and staff-tasks list URLs retain their scope and redirect to the canonical destinations. Staff task aliases preserve search and queue parameters.

## 2. One place to manage each subject

- Administration links to the existing directory and profiles. ERP invitations produce activation links; beta invitations retain the email flow. Status labels distinguish pending, joined, expired and revoked invitations.
- Each profile has one Login & access panel. Revoke role, Revoke company access and Deactivate employee remain distinct commands and retain their existing authority checks.
- The company level catalog and department heads live in Organization. A profile owns its current level, versioned change and level history. Scoped directory editors retain their existing permitted engagement-edit flow; company level managers use the dedicated level command.
- Task, leave and expense pages link to the selected approval item in Administration. The queue resolves authorization for the business record before reading its approval details. Rerouting uses the original command, record version and approval instance version.
- Invitation lists and file usage are filtered by company. The existing beta settings response now includes attached file usage; no new write API was introduced.

## 3. Daily work

- Home shows at most three next steps. Composer and feed comments start collapsed. Comments remain expanded on post pages. Follow, summary and report are secondary post actions.
- Composer, post actions, comments and task preparation have separate pending/error state. AI comment selection is in the task dialog. Manual fields and drafts survive failures and collapsing the UI; failed commands retain their retry key.
- Work combines tasks, leave, expenses, counts and transfers in one `WorkItem` view, with queue/type/search URL state. New exposes supported creation flows according to server permissions; beta starts from a conversation.
- Inbox contains actual notifications, with Unread and All views. Leave requests are no longer injected into the inbox.
- Approval evidence and blockers remain visible. History and technical versions are collapsed. Mobile leave actions leave the four main destinations accessible.

## 4. ERP consistency and retirement

- Directory and search exclude inaccessible capabilities. Accessible modules are retained regardless of a registry preview label.
- Module workspaces retain their nine layouts. Secondary filters, metrics and module context are collapsed, with one primary create action. Finance overview links to four canonical destinations.
- People opens the directory. Payroll is a separate authorized tab. Search stays visible while secondary people filters are collapsed.
- Social/auth/settings styles are scoped CSS Modules, using shared colors, buttons and dialogs. Mobile icon controls and main actions have a minimum 44px target.
- Removed the unused legacy SocialFeed component, unused BetaWork component and their unimported page styles. Prototype, registry, fixtures and seed/test inputs remain.

## Validation and review artifacts

Automated validation includes permission-aware navigation, Administration section boundaries, existing command concurrency/version/history tests, real local task and leave workflows, AI failure fallback, profile level/access workflows, URL/Back behavior and read-only previews.

Visual evidence is stored locally in `.scratch/ux-simplification/before` and `after`. The after matrix covers nine workspace layouts plus Home, Work, People, Inbox, Organization and Approvals at 390, 768 and 1440 pixels in light and dark themes. Tests assert no page-level horizontal overflow. Navigation tests cover keyboard focus, Escape, search and persisted appearance.

The pre-existing working tree contained substantial unrelated changes. This implementation preserves them and is not committed. `.scratch/ux-simplification/patches/00-complete.patch` compares only this delivery against its starting snapshot and supports a complete rollback without resetting the repository to HEAD. Four sequential source patches in `patches/staged/01.patch` through `04.patch` separate the navigation foundation, administration consolidation, daily work and ERP cleanup. Every cumulative stage passes TypeScript in an isolated copy. Apply in order; rollback the latest stage first. The first stage deliberately retains the legacy settings controls until the administration migration in stage two. The complete patch also includes the documentation updates.

Final verification:

- ESLint passes. Next.js production build and OpenNext Cloudflare bundle both pass (`bun run build:cloudflare` executes the Next.js build).
- Integration: 13 files, 81 tests pass.
- The final aggregate E2E run passed 30 scenarios and skipped four viewport-independent duplicates. Two navigation waits timed out under local development load: the streamed Home body exceeded a five-second assertion, and the Work form response took 9.87 seconds against a ten-second action deadline. The tests now wait explicitly for the destination content for up to 20 seconds; assertions and business operations are unchanged. All four targeted reruns (both affected scenarios on both viewports) then passed. Across the aggregate run and targeted reruns, all 32 executed scenarios pass: 18 desktop and 14 mobile.
- The profile scenario exercises level-change retry keys/history, activation links, role revocation, company revocation and deactivation/reactivation without restoring access. Rerouting exercises an actual concurrent version change, retained form input, refresh, successful replacement and historical evidence on both viewports.
- Final visual smoke: 42 additional screenshots, seven daily/admin routes across all three widths and both themes. No horizontal overflow; visible main/header/navigation buttons at 390px measure at least 44×44px. Usage data loads successfully.

Logs are collected in `.scratch/ux-simplification/verification`. Local fixture profiles are archived and test approval work is cancelled when its scenario completes.

Live email delivery and Stripe checkout are not exercised by the local UI suite. Existing beta limits and production readiness boundaries remain as documented in the social beta and global core plans.
