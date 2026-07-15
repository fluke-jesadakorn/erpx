# Folio — AI-Augmented Finance System

Local, single-tenant AI ERP for expense capture, multi-stage approval, purchase orders, double-entry GL, and executive reporting. The stack runs side-by-side with the shared local infrastructure (PostgreSQL 18 with pgvector, n8n, MinIO, local Ollama) provisioned in the parent repo.

---

## Architecture

Three local services + one database. The AI pipeline + OCR + RBAC matrix have been promoted out of the Next.js process into dedicated Fastify services for performance, isolation, and persistence.

| Service      | Stack                                 | Default Port |
|--------------|---------------------------------------|--------------|
| web-admin    | Next.js 16 (App Router) + React 19 + Tailwind v4 | 3003 |
| rbac-svc     | Fastify + pg                          | 3100 |
| ai-svc       | Fastify + multipart + sharp + pg + MinIO + Ollama | 3004 |
| ocr          | merged into ai-svc                    | (legacy alias 8765 dropped) |
| postgres     | PostgreSQL 18 + pgvector + pgcrypto   | 5432 |
| minio        | MinIO (slips bucket: `folio-storage`)| 9000 (api) / 9001 (console) |
| ollama       | Ollama daemon                         | 11434 |

Slip storage moved from local filesystem to MinIO. Pre-signed URLs are issued by ai-svc and served directly to the browser.

```
Browser → web-admin (Next.js) ─┬→ rbac-svc (Fastify :3100) ──→ Postgres
                               └→ ai-svc   (Fastify :3004) ──┬→ Postgres
                                                              ├→ Ollama
                                                              └→ MinIO (folio-storage)
```

Browser → web-admin's `/api/*` routes are **thin HTTP proxies** to rbac-svc / ai-svc. Server actions call ai-svc directly via the `lib/ai/router.ts` shim.

---

## Project Layout

```text
folio/
├── README.md
├── package.json                 # Top-level utility scripts (embed, audit)
├── db/                          # SQL migrations + seed scripts + AI provider seeds
│   ├── init.sql                 # v1 schema (users, roles, COA, expenses)
│   ├── seed.sql                 # v1 seed
│   ├── add_v2.sql … add_stage_override_audit.sql
│   ├── embed_coa.js             # Embed chart-of-accounts via Ollama bge-m3
│   ├── seed_ai_settings.js
│   ├── audit_ai_coverage.js
│   └── rbac/                    # RBAC schema (rbac.* namespace)
├── server/
│   ├── rbac-svc/                # Fastify :3100 — RBAC matrix + groups + audit + export
│   └── ai-svc/                  # Fastify :3004 — AI router + OCR + MinIO + Slip storage
└── app/
    ├── .env.example
    ├── .env.local
    ├── package.json             # Next.js dependencies
    ├── src/
    │   ├── app/
    │   │   ├── page.tsx         # Tiles page (role-aware)
    │   │   ├── dashboard/       # Role-specific dashboards
    │   │   ├── [slug]/page.tsx  # Tile workbenches
    │   │   ├── org-chart/       # Org Chart
    │   │   └── api/             # thin HTTP proxies → rbac-svc / ai-svc
    │   │       ├── actor, upload, slips/file, notifications (in-process data)
    │   │       ├── can, can-batch, matrix, cells, groups, modules, roles, audit, export, org, tiles → rbac-svc
    │   │       └── ai/{invoke,providers,models,assignments,staff,invocations,sections} → ai-svc
    │   ├── components/          # UI components (org-chart, workspaces, tiles, ai, hr)
    │   └── lib/
    │       ├── permissions.ts   # Type aliases only (legacy module)
    │       ├── policy/engine.ts # Pure approval policy matcher (used by server actions)
    │       ├── ai/router.ts     # Thin HTTP client to ai-svc (for server actions)
    │       ├── server/          # guard / queries / actor (Next.js only — session + actor)
    │       └── …
    └── tests/                   # .test.mjs files (policy, org scope, inheritance)
```

---

## Setup Runbook

### 1. Provision the database

```bash
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/init.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_v2.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_po.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_hr.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_approval_chain.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/apply_staff_level.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_ai_settings.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_ai_section_health.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_stage_override_audit.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/add_access_requests.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/seed.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/v2_seed.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/seed_gl_mock.sql
```

For perm schema (replaces legacy `db/rbac/`):

```bash
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/perm/9000-rebuild-string-grammar.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/perm/9001-seed-new-grammar.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/perm/9002-seed-user-roles-and-depts.sql
PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db -f db/migrations/2026-07-17-active-permissions-view.sql
```

### 2. Create MinIO bucket for slips

```bash
# Use mc or the MinIO console at http://localhost:9001
# Bucket: folio-storage
```

### 3. Embed chart-of-accounts (Ollama bge-m3)

```bash
cd world-erp
bun install
bun run embed
```

### 4. Seed AI providers, models, staff, assignments

```bash
cd world-erp
node db/seed_ai_settings.js
```

### 5. Configure services

Each service reads from a `.env` file (or environment):

- `folio/app/.env.local` — web-admin secrets
- `folio/server/rbac-svc/.env` — rbac-svc secrets
- `folio/server/ai-svc/.env` — ai-svc secrets

All three share the same `SESSION_SECRET` (HMAC-SHA256 signing key for the `folio_session` cookie). Generate with `openssl rand -hex 32`.

### 6. Install deps + start services

```bash
# rbac-svc (Fastify :3100)
cd folio/server/rbac-svc
bun install
bun run dev          # http://localhost:3100/health

# ai-svc (Fastify :3004) — includes Swift N-API Vision addon
cd folio/server/ai-svc
bun install
bun run install:vision-ocr   # builds the Swift N-API macOS Vision addon (mac only)
bun run dev                 # http://localhost:3004/health

# web-admin (Next.js :3003)
cd folio/app
bun install
bun run dev
```

Open the dashboard at `http://localhost:3003`.

---

## Migration from old architecture

- **Old**: One Next.js process handled UI + RBAC matrix + AI router + business queries + slip file serving. OCR was a separate Fastify process on `:3004` + legacy alias `:8765`. Slips were on the local filesystem under `folio/uploads/`.
- **New**: Three services (`web-admin`, `rbac-svc`, `ai-svc`). `ai-svc` absorbs OCR + AI + storage. RBAC matrix lives in its own persistent process. Slips go to MinIO. The legacy `folio/ocr/` and `folio/uploads/` directories are gone.

The `folio/n8n/` folder is historical; the deprecated `n8n/` flows are no longer used.

---

## See also

- AGENTS.md (workspace root) — shared conventions
- `infra/` — shared host-native services (Postgres, MinIO, Ollama, n8n, launchd plists)