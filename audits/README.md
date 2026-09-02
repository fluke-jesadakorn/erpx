# audits — Evidence (archived)

> **Archived 2026-09-02.** `330 dirs / 72M → 22 dirs / 3.9M` pruned at `fe6c286e`+`38d4bf4e` (300 G* + web-e2e 49M removed). Full evidence moved to `docs/archive/dioxus-2026-08-evidence/` (22 dirs, 3.9M) + `backup/rust-2026-09-02:audits` (330 dirs, 72M).

- **Before (backup):** `git checkout backup/rust-2026-09-02 -- audits/` (330 dirs, 72M, `finance_darkmode`×2 etc)
- **After (archive):** `docs/archive/dioxus-2026-08-evidence/` (22 dirs: `CURRENT-CERTIFICATION.md`, `DIOXUS-CUTOVER*`, `ADR-017-PHASES`, `e2e-all-modules`, `finance_darkmode_fixed`, `finance_live_full` etc) — see `docs/archive/dioxus-2026-08-evidence/README.md`
- **Living:** `npm run lint` + `build` (this branch); Rust `cargo xtask verify` on `backup`

Do not add new evidence here — put Next.js evidence under `docs/archive/` or new `audits/nextjs-*` when `src/app/api` lands.
