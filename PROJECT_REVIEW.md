# ERPX — Project Review

Prepared as an architecture-review brief from the checked-in source and documentation. This is a system summary, not a security audit or a claim that every rollout gate has passed.

## 1. Executive Summary

ERPX is a multi-company ERP and workplace platform for employees, managers, and approvers working across finance, people operations, sales, procurement, inventory, hospitality, service, and related functions. It also provides customer-facing self-service and a separate platform-administration surface.

The central vision is a connected workplace with clear product ownership: users enter through a shared Apps experience, select an authorized Company and work context, then complete work in product-specific areas. Business modules own their rules and records; cross-product workflows use explicit handoffs, shared identity and scoped evidence. The repository describes 15 products, 33 modules, and shared/public surfaces.

The current codebase is a Next.js rebuild of a prior Rust/Dioxus system. The modular boundaries and PostgreSQL transaction model are substantial, but the delivery plan still treats product migration, acceptance, and legacy-console retirement as gated work. Dated engineering receipts apply to the exact source they recorded and should not be read as blanket acceptance of later changes.

## 2. Tech Stack

- **Web:** Next.js 16 App Router, React 19, TypeScript, and Tailwind CSS 4; standalone Node.js release builds.
- **Runtime and tooling:** Node.js 26 for new development/releases, Bun 1.4 for package and test workflows. The checked-in operational docs describe an older installed production runtime pending a reviewed release.
- **Data:** PostgreSQL 18, the `pg` driver, and Drizzle ORM for authentication/schema integration. Application-owned SQL migrations are split across control-plane and Holding read/write tracks.
- **Identity and validation:** Better Auth and Zod.
- **Files and supporting services:** Private file-store adapters, with an S3-compatible MinIO adapter available; Redis cache adapter; local malware scanning and document/image preview tooling for attachment flows.
- **AI:** Company-scoped provider configuration and custom HTTP transport for chat-completions, Responses, and Messages-style protocols. MCP server/client libraries expose the Executive integration. No first-party model-vendor SDK is listed as a runtime dependency.
- **Quality:** Vitest, Playwright, ESLint, TypeScript, and repository-specific architecture, structure, catalogue, and documentation checks.

## 3. Project Structure

```text
.
├── docs/                         # ADRs, contracts, plans, evidence, testing
├── fixtures/<owner>/             # Seeds and test builders by business owner
├── migrations/postgres/
│   ├── control/                   # Identity, platform, and billing writes
│   ├── control-read/
│   ├── holding-read/
│   └── holding-write/             # Company business records and workflows
├── ops/                           # Runtime, deployment, and incident procedures
├── public/                        # Brand, fonts, and static assets
├── scripts/
│   ├── architecture/ docs/ local/
│   ├── performance/ seed/ testing/ ui/
├── src/
│   ├── app/                       # App Router pages and Route Handlers
│   ├── components/<owner>/        # Product and shared UI
│   ├── data/                       # Product registries and generated inventories
│   ├── lib/<owner>/                # Client/presentation helpers by owner
│   ├── server/
│   │   ├── modules/<owner>/        # domain, application, adapters, public API
│   │   └── runtime/                # Compatibility bootstrap
│   └── workers/                   # Compatibility exports for workflow jobs
└── tests/
    ├── catalogue/ config/ support/
    ├── unit/ integration/ provider/
    └── e2e/ journeys/ uat/
```

## 4. Core Architecture & Workflow

ERPX is a modular monolith. Next.js pages and Route Handlers authenticate requests and resolve the active account, Company, and role/scope. UI and route code call each owner module through named `public/` exports; each module separates domain rules, application workflows, and infrastructure adapters.

```text
Browser
  → Next.js App Router (pages and Route Handlers)
  → session + Company/role context
  → owner public API → domain/application workflow
  → PostgreSQL control or Holding database
       ├─ private file/object storage for file bytes
       └─ transactional outbox → Node jobs worker

AI request
  → authorization + agent governance
  → scoped owner-source adapters
  → quota/audit gateway + bounded provider transport
  → source/authority recheck → stored response and usage evidence
```

The control plane stores account, platform, and Holding-binding data. Each Holding has separate read and write database bindings; business data is scoped further by Company, branch, department, assignment, and current authorization. The operational contract places business writes, authorization assertions, command receipts, audit records, and outbox events in a serializable transaction. Read projections are rebuildable. A Node jobs process claims leased outbox work, retries failures, and records terminal failures for review.

File metadata and ownership are recorded in PostgreSQL while content is kept behind private file-store adapters. Intake is authorization- and purpose-scoped, with malware scanning and immutable source/binding evidence.

AI is an assisting workflow, not a separate business authority. The Agent module selects an approved Company/deployment provider, applies source allowlists and usage limits, fetches minimal evidence through owner adapters, and calls the provider through bounded transport that validates destination addresses and limits response size. It records usage and governance evidence and rechecks authority/source versions before completing a response. The workspace assistant is explicitly read-only; business changes remain owner commands with their normal permissions and review steps. Provider credentials are encrypted at rest. The Executive MCP endpoint uses the same scoped Executive sources and authorization boundary.

## 5. Key File Highlights

### Main application entry

`src/app/page.tsx` resolves the signed-in user to a saved or currently authorized workspace; the root layout in `src/app/layout.tsx` installs shared account, context, time, and attachment providers.

```tsx
export default async function RootPage({ searchParams }: PageProps<"/">) {
  const session = await (await createAuth()).api.getSession({ headers: await headers() });
  if (!session) redirect("/login");

  // Prefer the saved workspace, but verify it against current account grants.
  const saved = await savedWorkspaceForPresentation(session.user.id);
  // ...resolve the remembered destination and verify current grants...
  for (const binding of await activeHoldingBindings()) {
    const workspace = await firstAuthorizedWorkspace(binding.slug);
    if (workspace) redirect(workspaceHref(workspace));
  }
  redirect(canonicalUiHref("/holdings/erpx/manage-erpx"));
}
```

### Container configuration

No `docker-compose.yml`, Compose file, or Dockerfile was found in the repository. The documented deployment is a host-managed standalone Node service with PostgreSQL and private runtime services. A reproducible containerized development and deployment path would be useful before inviting external contributors.

### Database core model

The baseline in `migrations/postgres/holding-write/0001_baseline.sql` demonstrates scoped assignments and transaction evidence. The excerpt is shortened to the architectural fields; it contains no deployment credentials or sample account data.

```sql
CREATE TABLE assignments (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  role TEXT NOT NULL,
  company_id TEXT NOT NULL,
  branch_id TEXT,
  valid_from BIGINT NOT NULL,
  valid_until BIGINT NOT NULL,
  revoked_at BIGINT,
  version BIGINT NOT NULL DEFAULT 1,
  scope_kind TEXT NOT NULL DEFAULT 'holding'
    CHECK (scope_kind IN ('holding', 'branch', 'department'))
);

CREATE TABLE command_receipts (
  command_id TEXT PRIMARY KEY,
  operation TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  record_id TEXT NOT NULL,
  result_json TEXT,
  created_at BIGINT NOT NULL
);
```

The broader schema includes audit events, decision evidence, attachment ownership, and an outbox, alongside domain tables for finance, inventory, people, hospitality, sales, and other products. The primary PostgreSQL adapters live under `src/server/modules/runtime/`; `src/server/runtime/` remains a compatibility bootstrap. A separate control schema owns accounts, platform assignments, invitations, and Holding bindings.

## 6. Current README / Roadmap

`README.md` identifies the active stack, repository layout, local workflows, test commands, and recovery boundary for the archived Rust system. It points to `docs/production-plan.md` as the active delivery plan and `docs/TOC.md` as the documentation hub. The docs are unusually detailed: ADRs, current contracts, generated inventories, dated evidence, operator procedures, and migration checkpoints are all present.

The roadmap is still an internal migration/acceptance plan rather than a public product roadmap. It records 33 modules and 187 capabilities, while current documentation says product acceptance and Console-to-Apps retirement remain gated. Historical evidence is explicitly source- and date-bound, so public status should be distilled into a simple current/remaining table.

Before open-sourcing, the repository would benefit from:

- A supported, reproducible setup for a clean Linux/macOS checkout, including PostgreSQL, the jobs worker, file storage, and attachment scanning/preview dependencies. There is currently no Compose-based bootstrap.
- A concise public architecture and deployment guide separated from operator-specific machine/runtime procedures and internal migration evidence.
- A maintained setup/configuration reference with placeholder-only examples and a clear distinction between required, optional, and test-only services.
- A public roadmap that distinguishes implemented behavior, verified behavior, and pending acceptance without requiring readers to reconcile many dated checkpoints.
- A license, contribution guide, security reporting policy, and code-of-conduct document; none is present at the repository root.
- A pre-release repository/data hygiene review for private infrastructure details, test fixtures, generated artifacts, and all credential-bearing configuration, followed by a clean-checkout CI path that proves the documented workflow.
