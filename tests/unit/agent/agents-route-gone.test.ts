import { expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const src = (path: string) => fileURLToPath(new URL(`../../../${path}`, import.meta.url));
const read = (path: string) => readFileSync(src(path), 'utf8');

const handler =
  'packages/feature-admin/src/routes/api/v1/holdings/[holdingSlug]/agents/route.ts';
const reexports = [
  'apps/admin/src/app/api/v1/holdings/[holdingSlug]/agents/route.ts',
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/agents/route.ts',
];

it('retires the Next agents handler and every app re-export', () => {
  expect(existsSync(src(handler))).toBe(false);
  for (const file of reexports) expect(existsSync(src(file)), file).toBe(false);
});

it('serves the full agents workspace from the owner-native GET read', () => {
  const ingress = read('packages/server-agent/src/public/agents_public_read.rs');
  expect(ingress).toContain(
    '.route("/api/v1/holdings/{holding_slug}/agents", get(read))',
  );
  expect(ingress).toContain('include_evidence: true');
  expect(ingress).toContain('AgentWorkspaceHttpKind::Workspace');
  expect(ingress).toContain('serde_json::json!({"data": value})');

  const application = read('packages/server-agent/src/application/workspace_read.rs');
  expect(application).toContain('pub async fn agent_workspace_read');
  const postgres = read('packages/server-agent/src/adapters/workspace_read_postgres.rs');
  expect(postgres).toContain('PgAgentWorkspaceRead');
});
