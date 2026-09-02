# ops — Runbooks & Observability (Next.js era)

> **Dioxus era archived 2026-09-02** (`docs/archive/ops-dioxus-2026-09-02/` — 68K, runbooks `:3004/:50051`, alerts `erpx_projection_*`). Living Next.js runbooks are here.

- **Incident:** `ops/runbooks/nextjs-incident.md` — Vercel/Next.js 16, `npm build` failures, DB pool via Prisma/Drizzle
- **Recovery:** `ops/runbooks/nextjs-recovery.md` — Postgres PITR via Neon/Supabase, object R2/S3 recovery
- **Alerts:** `ops/observability/nextjs-alerts.yml` — `nextjs_http_requests_total`, `build_failures`, `db_pool` (not `erpx_projection_*`)
- **Evidence:** `ops/web-production-evidence.example.json` — REQ-* schema (stack-agnostic, `not_executed` placeholders)

Archived Dioxus: `docs/archive/ops-dioxus-2026-09-02/` (runbooks `Deprecated`, alerts `erpx_*`) + `backup/rust-2026-09-02:ops`
