# audits — Evidence Index (Next.js era)

> **Pruned 2026-09-02 — 330 dirs / 72M → 22 dirs / 3.9M.** Full history remains on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`, `f97b74d6`). This README is the single index; do not add evidence without updating it.

## Before / After

- **Before:** 330 top-level entries (322 dirs + ~8 files), 700 files recursive (324 md + 340 png + 23 json), 72M. Heavy duplication: `finance_darkmode` ≡ `finance_darkmode_fixed` (2.7M×2), `finance_images` 852K, 4× `finance_live*` sets, `web-e2e` 49M, `approval-campaign-to-cash-design` 3.2M, `APPROVAL-GOVERNANCE-PENCIL` 2.6M, `*.log` (431K) + `.DS_Store`, ~200 old `G0..G11` gate dirs.
- **After:** 22 dirs retained (see below) + 8 files, 3.9M (<20M). Rest archived on backup.

## What remains on this branch (22 dirs/files)

- `CURRENT-CERTIFICATION.md` — last Dioxus-era certification (banner: archived, Next.js gate is `npm run lint` + `build`)
- `DIOXUS-CUTOVER-2026-08-18.md` + `DIOXUS-TRACEABILITY-2026-08-18.md` + `DIOXUS-NATIVE-MACOS/` — Dioxus cutover evidence (archived)
- `ADR-017-PHASES-0-6-2026-08-22/` — collaboration/AI phases 0–6
- `e2e-all-modules/` — end-to-end evidence (8K) — `web-e2e/` 49M removed
- `finance_darkmode_fixed/` (2.7M, keep) — `finance_darkmode/` removed as duplicate
- `finance_live_full/` (1.0M, keep) — `finance_live/` / `finance_live_auth/` / `finance_live_real/` collapsed to 1
- `finance-e2e-2026-08-30.md` (13K) — finance flow evidence (keep md, `*.log` removed)
- `APPROVAL-GOVERNANCE-PENCIL` 2.6M + `approval-campaign` 3.2M — removed (not in keep-list)
- `COORDINATOR-CONTINUATION-2026-07-27/`, `COORDINATOR-FINAL-CERTIFICATION-2026-07-30/`, `COORDINATOR-LOCAL-ACCEPTANCE-2026-07-28/` — 3 representative
- `DIRECTORY-AUTH`, `DIRECTORY-AUTH-PREFLIGHT-2026-07-28`, `DIRECTORY-INTEGRATION`, `DIRECTORY-RBAC`, `DIRECTORY-SYNC` — 5 directory evidence
- `DEVELOPMENT-WARM-LOOP-2026-08-10.md`, `LOCAL-DATA-PLANE-CUTOVER-2026-08-01.md`, `WASM-SPLIT-HOT-PATCH-BLOCKER-2026-08-10.md`, `cqrs-ddd-refactor-baseline.md` — 4 small md
- (deleted) `G0..G11` old gates (~200 dirs, 300 total), `finance_images/` dup, 3× `finance_darkmode`/`finance_live*` dup, `web-e2e` 49M, 2× approval 5.8M, `*.log`, `.DS_Store`

## Prune rules (ticket 05)

1. Keep `_fixed` over un-suffixed dup (dedup `finance_darkmode`).
2. Delete `*.log` / `.DS_Store` (violates `.gitignore`).
3. Delete `G0..G11` old gates (~300 total) — full history on backup (see `git show fe6c286e --stat | grep audits` deleted 644 files).
4. Collapse `finance_live*` ×4 → `finance_live_full` only; remove `web-e2e` 49M from keep-list.

## Recovery

```bash
git checkout backup/rust-2026-09-02
ls audits/ | wc -l        # 330 on backup
ls audits/ | grep -E "G0|finance_darkmode" | head
git show rust-archive-2026-09-02 --stat | grep audits
du -sh audits             # 3.9M on development (was 72M)
```

## Edit rule

Every new evidence dir must be listed here. Do not reintroduce `*.log`, `web-e2e` 49M, or duplicate `finance_darkmode`.

## Cutover note

Before 2026-09-02, `CURRENT-CERTIFICATION.md` claimed `cargo xtask verify` PASS. That gate is now on backup; the living gate on `development` is `npm run lint` + `npm run build` (see `AGENTS.md`, `CUTOVER.md` 2026-09-02).
