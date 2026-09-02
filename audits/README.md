# audits — Evidence Index (Next.js era)

> **Pruned 2026-09-02 — 330 dirs / 72M → ~40 dirs / <20M.** Full history remains on `backup/rust-2026-09-02` (`rust-archive-2026-09-02`, `f97b74d6`). This README is the single index; do not add evidence without updating it.

## Before / After

- **Before:** 330 top-level entries (322 dirs + ~8 files), 700 files recursive (324 md + 340 png + 23 json), 72M. Heavy duplication: `finance_darkmode` ≡ `finance_darkmode_fixed` (2.7M ×2), `finance_images` 852K, 4× `finance_live*` sets, `*.log` (431K `finance-e2e-2026-08-30.log` despite `.gitignore:*.log`), `.DS_Store`, ~200 old `G0..G11` gate dirs.
- **After (target):** ~40 dirs retained + `CURRENT-CERTIFICATION.md`, `DIOXUS-CUTOVER*`, `ADR-017-PHASES`, `e2e-all-modules`, `finance_darkmode_fixed`, `finance_live_full` — rest archived on backup. Size <20M.

## What remains on this branch (~40 dirs)

- `CURRENT-CERTIFICATION.md` — last Dioxus-era certification (banner: archived, Next.js gate is `npm run lint` + `build`)
- `DIOXUS-CUTOVER-2026-08-18.md` + `DIOXUS-TRACEABILITY-2026-08-18.md` + `DIOXUS-NATIVE-MACOS/` — Dioxus cutover evidence (archived)
- `ADR-017-PHASES-0-6-2026-08-22/` — collaboration/AI phases 0–6
- `e2e-all-modules/`, `web-e2e/` — end-to-end evidence
- `finance_darkmode_fixed/` (keep) — `finance_darkmode/` removed as duplicate
- `finance_live_full/` (keep) — `finance_live/` / `finance_live_auth/` / `finance_live_real/` collapsed to 1
- `finance-e2e-2026-08-30.md` — finance flow evidence (keep md, `*.log` removed)
- `APPROVAL-GOVERNANCE-PENCIL-2026-08-13/`, `COORDINATOR-*/`, `DIRECTORY-*/` (4) — representative governance/directory evidence (keep)
- (delete) `G0..G11` old gates (~200 dirs), `finance_images/` dup, `finance_darkmode/` dup, `*.log`, `.DS_Store`, 4× `finance_live*` collapsed

## Prune rules (ticket 05)

1. Keep `_fixed` over un-suffixed dup (dedup `finance_darkmode`).
2. Delete `*.log` / `.DS_Store` (violates `.gitignore`).
3. Delete `G0..G11` old gates except the 6 listed above — full history on backup.
4. Collapse `finance_live*` ×4 → `finance_live_full` only.

## Recovery

```bash
git checkout backup/rust-2026-09-02
ls audits/ | wc -l        # 330 on backup
ls audits/ | grep -E "G0|finance_darkmode" | head
git show rust-archive-2026-09-02 --stat | grep audits
```

## Edit rule

Every new evidence dir must be listed here. Do not reintroduce `*.log` or duplicate `finance_darkmode`.

## Cutover note

Before 2026-09-02, `CURRENT-CERTIFICATION.md` claimed `cargo xtask verify` PASS. That gate is now on backup; the living gate on `development` is `npm run lint` + `npm run build` (see `AGENTS.md`, `CUTOVER.md` 2026-09-02).
