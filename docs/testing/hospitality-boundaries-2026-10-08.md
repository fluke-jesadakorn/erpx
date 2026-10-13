# Hospitality module boundaries — development verification, 2026-10-08

Scope: [ADR-040](../adr/ADR-040-hospitality-module-ownership-and-guest-accounts.md), [PMS contract](../contracts/next/pms.md), [POS contract](../contracts/next/pos.md). This is focused implementation evidence, not full browser/business release acceptance.

## Implemented

- Canonical Guest Account / บัญชีผู้เข้าพัก names, IDs, record kinds, routes and commands; central historical payload/URL/receipt adapter. Issued snapshots and audit/receipt bytes remain immutable.
- Additive migrations 1026–1027 applied only to the three development Holdings. Existing custom mixed roles retain approved grants; operational configuration maps to the new accounting grants. Twenty-four role revision changes are recorded in `.scratch/dev/hospitality-permission-report.json`.
- Separate PMS and POS operational approval from Finance reconciliation. Selected assignment and Company/Branch guards remain authoritative, including historical replay after the IAM context revision migration.
- Linked restaurant room charging through the public PMS boundary. Frozen correction request, independent POS decision, independent PMS decision, version checks and atomic money/stock completion. Pending requests have no financial or inventory effects.
- Exact hotel links for Night Audit and sale locks; independent shops retain their own day. Finance hospitality evidence, mapping, bank verification, reporting and settlement use Finance routes.
- Current browser fixtures use the explicit hotel link and independent accounting approval. Their full desktop/mobile lane has not been rerun in this work.

## Executed checks

| Check | Result |
| --- | --- |
| Focused native suites | 410 unique cases passed across 17 files: original 166 unit + 203 PostgreSQL cases, then 41 additional unit cases. POS 80 cases passed again after linked-job scoping changes. |
| Migration upgrade | Original IDs, balances, posted journals, frozen snapshots/receipts and converted custom permissions preserved; mutable JSON ordering excluded from byte identity assertions. |
| Reversal | Independent reviewers, pending uniqueness, stale source versions, rejection/new request, database evidence immutability, atomic rollback and replay covered. |
| Typecheck | Passed for application and engineering tests. |
| Lint | Passed with 0 errors; 3 unused-variable warnings remain. |
| Architecture and terminology | Passed; 0 exact legacy dependency edges. Terminology check is part of `architecture:check`; immutable history and central compatibility are explicit exclusions. Unrelated candidate portfolio vocabulary is preserved. |
| Isolated release build | Passed using Bun 1.2.17 / Node 24.20.0 in `.runtime/build`, without touching the development Next output or deploying. Final release path is in `.scratch/dev/guest-account-build-final-source.log`. |
| Public Console browser | Authenticated Guest Account approvals and Finance restaurant accounting rendered. Finance Guest Account evidence showed the same THB 1,800 balance and original posted journal. Historical URL redirected to canonical Guest Account URL with selectors retained. |
| POS Front browser | Existing authenticated session rendered sale controls and the Thai Guest Account reversal review queue; closed-shift restrictions visible. |
| PMS Front browser | Login required. Full role/assignment browser matrix and approval transactions remain unverified. |
| Drawer interaction | The existing Console form trigger did not produce a visible drawer through the browser tool. No form was submitted; drawer behavior remains unverified. |

Logs are private local artifacts under `.scratch/dev/guest-account-*.log`; earlier failed attempts are superseded only by the explicitly successful checks above. The 2026-10-03 engineering receipt retains its original date/source/checksums; exact original input files are archived, and only the renamed capability ID is projected into the current inventory. It grants no acceptance of this change.

## Environment availability

Development was restarted after a stalled route compile. It runs detached in tmux `erpx-guest-account-dev` on 3100 with PID/log records under `.scratch/dev/`. Development jobs were rebuilt from current source and consolidated from two duplicate processes into `erpx-guest-account-jobs`; all eight bindings were verified as `erpx_dev_*` before replacement. Development ClamAV definitions were refreshed with the installed official updater after the age check failed; scan protections and readiness thresholds were retained.

Local development, public [Console development](https://console-dev-erpx.jesadakorn.com/) and private production health each returned HTTP 200 after recovery. PostgreSQL remains on 5432. Private production was checked read-only. No staging access, production migrations or deployment occurred. Room charging remains gated for production pending acceptance.
