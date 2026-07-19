# Folio — AI-Augmented Finance System

Local, single-tenant AI ERP for expense capture, multi-stage approval, purchase orders, double-entry GL, HR, Law RAG, and executive reporting. Runs side-by-side with the shared local infrastructure (PostgreSQL with pgvector, n8n, MinIO, local Ollama) provisioned in the parent repo.

---

## Architecture

One Next.js process on port 3004. All RBAC, AI routing, OCR pipeline, slip storage, HR, Law, and Finance logic live inside the same Node.js process and are imported by route handlers via path aliases. The Fastify micro-services from earlier versions have been folded into the same process.

```
Browser → folio (Next.js 16 :3004, Turbopack) ─┬→ @/perm/*             ──→ Postgres
                                              ├→ @/ai/*                ──┬→ Postgres
                                              │                         ├→ Ollama
                                              │                         └→ MinIO (folio-storage)
                                              ├→ @/hr/*, @/law/*, @/finance/*
                                              └→ @/waybill/*           (consolidated Expense/PR/PO)
```

Anything imported via `@/...` is the canonical domain library. Anything under `app/` is the Next.js router (pages, route handlers, server actions). Anything under `components/` is shared UI.

---

## Project Layout

```text
folio/
├── README.md
├── package.json                       # single root manifest, no workspaces
├── next.config.ts                     # Turbopack resolveAlias → lib/components/app
├── tsconfig.json                      # paths: @/* → ./lib/*, @/components/* → ./components/*, @/app/* → ./app/*
├── eslint.config.mjs
├── postcss.config.mjs
├── proxy.ts                           # Next.js 16 proxy (HMAC session verification)
├── .env, .env.local, .env.example
├── app/
│   ├── (app)/                         # Top-level public pages + login redirect
│   ├── (app)/(protected)/             # role-aware tiles (tiles, hr, law, expense, sales, …)
│   ├── api/                           # thin route handlers → call @/<domain>/*
│   └── actions/                       # 'use server' action modules (use server-only)
├── components/                        # shared React components
├── lib/
│   ├── ai/                            # AI router + provider clients (minimax, ollama, openai)
│   ├── customer/                      # customer master + product RAG embeddings
│   ├── dashboard/                     # dashboard widgets
│   ├── finance/                       # GL posting (expense/procurement/sales) + finance RAG
│   ├── hook/                          # LINE webhook ingest + replay
│   ├── hr/                            # HR feature (employees, leave, intents, agent)
│   ├── i18n/                          # bilingual message dictionaries
│   ├── law/                           # Law RAG (contracts, chunks, rag, queue)
│   ├── ledger/                        # finance commentary
│   ├── native/vision-ocr/             # Swift N-API macOS Vision addon (loaded at runtime)
│   ├── notifications/                 # notifications queries + events + recipients
│   ├── org/                           # org tree, scope, display, queries
│   ├── perm/                          # permission grammar, department roles, ranks, and policies
│   ├── po/                            # PO-from-invoice helper
│   ├── policy/                        # policy lint
│   ├── sales/                         # SO extraction helper
│   ├── server/                        # Next-only session/guard helpers
│   ├── slips/                         # receipt OCR pipeline + MinIO storage
│   ├── waybill/                       # consolidated expense/PR/PO object + signed event log
│   ├── chat/                          # chat history + streaming (used by AI components)
│   ├── hero.ts, theme.ts, tileOrder.ts# small UI helpers (server-only)
│   ├── db.ts, config.ts               # pg pool + env config
│   └── next-shim.d.ts                 # ambient next/headers + next/server stubs
├── tests/                             # .mjs pure-logic tests re-implementing prod modules
├── db/                                # current schema snapshot, reference seed, and DB utilities
├── docs/                              # architecture + setup notes
├── n8n/                               # DEPRECATED (kept for historical reference; flows moved into lib/hook)
├── sample/law/                        # legal PDFs for Law RAG demo
└── bun.lock                           # single lockfile (workspaces merged)
```

### Key @/ aliases (tsconfig.json + next.config.ts)

| Alias                 | Resolves to          | Used by |
|-----------------------|----------------------|---------|
| `@/<x>`               | `./lib/<x>`          | app + components + lib cross-imports |
| `@/components/<x>`    | `./components/<x>`   | app pages |
| `@/app/<x>`           | `./app/<x>`          | component → server action imports |

`@folio-lib/*` was the old cross-folder alias under the workspaces setup; it has been replaced with `@/*` (the standard Next.js style).

---

## Setup

### 1. Provision the shared infra

Postgres with pgvector, MinIO, Ollama, and n8n are launched by `launchd` plists in the workspace-root `infra/` folder. From the parent repo:

```bash
launchctl list | grep lawpoc
# Postgres on 5432 (folio_db), MinIO on 9000/9001, Ollama on 11434, n8n on 5678
```

### 2. Initialize a fresh database

```bash
bun run db:setup
```

`db/setup.sh` applies `db/schema.sql` and `db/seed.sql`. It only accepts a fresh database and refuses to modify one where `folio.users` already exists. The seed contains reference catalogs and access policy only; it does not contain users, business transactions, audit history, embeddings, or API keys.

### 3. Install deps and build COA embeddings

```bash
bun install
bun run embed    # node db/embed_coa.js — embeds chart-of-accounts via Ollama bge-m3
```

### 4. Configure env

- `.env` — top-level LINE / OpenRouter secrets (read by `bun run embed`/`bun run audit`)
- `.env.local` — Next.js runtime secrets (`SESSION_SECRET`, Postgres, MinIO, ENCRYPTION_KEY, etc.)
- `lib/config.ts` reads the same.

Generate the session secret with `openssl rand -hex 32`; both the cookie mint and the HMAC verifier use the same value.

### First-user bootstrap

The organization rebuild intentionally leaves no active identity. Set a strong, one-time `FOLIO_BOOTSTRAP_TOKEN` in `.env.local`, then create the first access administrator:

```bash
curl -X POST http://localhost:3004/api/auth/bootstrap \
  -H "x-folio-bootstrap-token: $FOLIO_BOOTSTRAP_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"employee_code":"BOOT-001","fullname":"Platform Administrator","department_id":"it","role_id":"it_manager"}'
```

The endpoint only succeeds while there are no active users, assigns the selected department-owned access-admin role, and closes permanently after the first user is created. Configure `FOLIO_WORKER_TOKEN` for the internal approver-nudge and Law indexing workers.

### 5. Start the dev server

```bash
bun run dev   # next dev --turbopack -p 3004 -H 0.0.0.0
```

Open `http://localhost:3004`.

### 6. Build & verify

```bash
bun run lint        # eslint (flat config; react-hooks set-state-in-effect + purity + refs disabled)
bun run typecheck   # tsc --noEmit
bun run build       # next build --turbopack (optional — validation only; clean .next/ before dev)
```

---

## Key subsystems

### RBAC (single source of truth)

`lib/perm/` defines the canonical permission string grammar:

```
<domain>:<subject>:<verb>[:<qualifier>]::<effect>
```

Hierarchy roles have stable IDs such as `accounting_manager` and a numeric `rank` in `perm.roles`; rank 1 is the highest authority. Each hierarchy role belongs to exactly one `perm.departments` row.

Caller rules:

- Server-only code: `import { ... } from '@/perm/server'` (or `@/hr/server`, `@/law/server`).
- Client code: `import { ... } from '@/perm'`.

### Waybill (consolidated Expense/PR/PO)

`lib/waybill/` is the single object representing any waybill regardless of origin (`expense` / `pr` / `po`). The `waybill_events` log is an append-only HMAC-SHA256-signed linked list. Mutations across the app call `appendWaybillEvent()` from `lib/waybill/append.ts`.

Stage names follow finance-standard keys — see `lib/waybill/labels.ts` for the bilingual stages and `lib/perm/stages.ts` for stage→role→permission mapping.

### HR / Law

- HR: `lib/hr/{agent,employees,leave,intents,state}` plus `app/api/hr/*` route handlers.
- Law: `lib/law/{admin,chunks,contracts,queue,rag}` plus `app/api/law/*` route handlers.
- Two distinct LINE OAs:
  - Folio / Law bot — `LINE_CHANNEL_SECRET` + `LINE_CHANNEL_ACCESS_TOKEN`, webhook → `app/api/hook/line-law`.
  - HR bot — `HR_LINE_CHANNEL_SECRET` + `HR_LINE_CHANNEL_ACCESS_TOKEN`, webhook → `app/api/hook/line-hr`.

### Native vision OCR

`lib/native/vision-ocr/` is a Swift N-API addon compiled only on macOS (`Sources/`, `binding.gyp`, `build/Release/vision_ocr.node`). The runtime loader at `lib/native/vision-ocr/index.js` resolves the `.node` binary via `createRequire` and degrades to a no-op on non-darwin or when the build is missing. The OCR pipeline in `lib/slips/ocrPipeline.ts` consumes Pass 3 (Swift Vision) when available.

`next.config.ts` ships `serverExternalPackages: ["lib/native/vision-ocr"]` so Next.js does not bundle the addon.

---

## See also

- `AGENTS.md` (workspace root) — shared conventions
- `infra/` — shared host-native services
- `docs/BUNDLER.md` — Turbopack notes
- `docs/LINE-SETUP.md`, `docs/N8N-SETUP.md` — webhook setup
- `docs/RBAC-STANDARD.md` — current organization and permission model
- `docs/ORG-ACCESS.md` — departments, ranks, and responsibility boundaries
- `docs/stage-approval-design.md` — finance-standard stage keys + Waybill pipeline
