# Next.js Production Incident Runbook

> **Living — Next.js 16 / Vercel.** Replaces `docs/archive/ops-dioxus-2026-09-02/runbooks/web-production-incident.md` (Dioxus `:3004`). See `docs/adr/ADR-020` + `docs/contracts/next/route-table.md`.

## Server unavailable (Vercel / self-hosted Next.js)

1. Check Vercel deployment status, `/_health` / `/_ready` (add if missing in `src/app/api/health`), build logs `npm run build` typecheck.
2. Compare deployment revision, `DATABASE_URL` pool health (Prisma `db.$queryRaw`), Edge/Node runtime.
3. Stop rollout. Redeploy last known-good revision; never run two schema migrations concurrently.

## HTTP errors

Group by route (`/api/sales/quotes`), status, `x-correlation-id`. Separate auth denials (NextAuth) from DB failures.

## Authentication failures

Check NextAuth/Clerk, secure-cookie, CSRF origin, rate-limit, account lockouts.

## Build failures

Check `next build` typecheck, `eslint` (`npm run lint`), missing `DATABASE_URL`, Prisma migrate diff.

## Evidence

Preserve `x-correlation-id`, audit events, REQ-* outcomes in `ops/web-production-evidence.example.json`.
