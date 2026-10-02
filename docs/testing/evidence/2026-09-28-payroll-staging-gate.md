# Payroll staging release gate · 2026-09-28

The Payroll implementation is running in the development workspace. The isolated staging instance has **not** been upgraded or used for Payroll acceptance.

## Verified development evidence

- Latest focused Payroll, roster, rules, and payment tests: 8 files, 45 tests passed. The integration flow also verifies that a posted payee snapshot remains payable after the HR name changes and formula-shaped names are escaped in review and bank CSV.
- `bun run typecheck` passed; `bun run lint` passed with one unrelated existing warning in `service-navigation.tsx`.
- `bun run deploy:build` passed after the People route and English-only Payroll refactor and produced release `2026-09-28T17-18-34-892Z`, schema SHA-256 `d8db70ca88676c147d41a4a7687cbce75e39f16ae3922a8dbd52c0d805e7173e`.
- Authenticated desktop and 390 px mobile checks on `console-dev-erpx.jesadakorn.com` showed the three-stage Payroll page, Settings/account mapping, and HR cost-assignment verification. A later authenticated pass confirmed English-only dashboard, settings, and run detail under `/people/payroll`, with Payroll in the People menu and the old `/payroll/settings` URL redirecting to the canonical route. Development Holding databases H001/H002/H003 have migrations 0352–0358.
- Final availability check: local/public dev, local/public staging, and private production `/api/health` each returned HTTP 200. The staging console Payroll route returned HTTP 404 on the old release.

## Staging difference requiring review

The installed staging release is `2026-09-25T11-06-09-644Z` with schema SHA-256 `f5d458825a2916b0b0c12c0f87a81de07ae72c0e3346bc36df8f0a7888723b0c`. Its holding-write migration inventory stops at 0304 (plus 0900), and its control inventory stops at 0018. Comparing migration files in the installed release against this workspace yields **68 new SQL files and one changed existing SQL file** (`holding-write/0293_executive_evidence.sql`). The new files include control 0019–0029, holding-write 0305–0358 and 0901–0902. Payroll is 0352–0358, but the complete delta also includes unrelated authority, social, attachments, billing, and other work.

`ops/macos/upgrade-front-staging.py` is a code-only installer and rejects a changed schema hash. The staging migration runbook at `ops/macos/billing-target-staging-migration.md` requires a reviewed full ordered delta, verified backup and restore rehearsal, then an isolated staging migration and activation. Applying only the Payroll files or pointing staging at the development process would violate that gate. The current staging Payroll URL returns a 404 on the old release.

## Next staging acceptance sequence

1. Review all 68 added and one changed migrations with the owners of the concurrent changes; resolve the changed 0293 file and freeze the release source.
2. Capture and verify a consistent staging backup, then rehearse the exact full migration order against isolated restored databases. Record DDL lock and rollback results.
3. Apply the reviewed migrations with staging migration credentials, activate the matching release, and verify `3103`, public host routing, app-role grants, scheduled jobs, and Payroll role isolation.
4. Complete one English desktop/mobile Payroll round with HR, an independent approver, Finance, and Treasury using synthetic data. Record expected/actual calculation and GL evidence.

Production statutory calculation, actual bank format, and payslip release remain gated by independently reviewed Thai expected results, an approved bank specification, and Payroll/Finance/release acceptance. The fixtures under `fixtures/payroll-thai/` are synthetic engineering examples only.
