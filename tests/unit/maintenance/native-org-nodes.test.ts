import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

import { ORG_NODES_SOURCE } from '../../../packages/server-organization/src/adapters/org-sources';

const slice = new URL(
  '../../../packages/server-maintenance/src/adapters/native_org_nodes/',
  import.meta.url,
);
const read = (name: string) => readFile(new URL(name, slice), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

/** The exact statement the TypeScript `nodes` repository branch composes from
 * the Organization owner projection and the maintenance read-tail head. The
 * native slice must execute this literal with six positional binds, not a plan
 * or a TypeScript-prepared query. */
const oracle =
  `SELECT id,kind,display_name,branch_id FROM (${ORG_NODES_SOURCE}) n ` +
  "WHERE company_id=$1 AND status='active' " +
  "AND kind IN ('branch','headquarters','department') " +
  'AND ($2::text IS NULL OR id=$3 OR branch_id=$4) ' +
  'AND ($5::text IS NULL OR id=$6) ORDER BY kind,display_name';

async function rustSql(): Promise<string> {
  const text = await read('domain.rs');
  const match = text.match(/ORG_NODES_SQL: &str = "([\s\S]*?)";/);
  if (!match) throw new Error('ORG_NODES_SQL constant not found');
  return match[1]
    .split('\\\n')
    .map((line) => line.trim())
    .join(' ')
    .replace(/\\"/g, '"');
}

describe('native Maintenance organization-node read parity', () => {
  it('executes the same composed statement as the TypeScript authority', async () => {
    const sql = await rustSql();
    expect(sql).toBe(oracle);
    expect(sha(sql)).toBe(sha(oracle));
    for (let n = 1; n <= 6; n += 1) expect(sql.match(new RegExp(`\\$${n}`, 'g'))).toHaveLength(1);
    expect(sql).not.toContain('?');
    for (const forbidden of ['INSERT', 'UPDATE ', 'DELETE ', 'LOCK ']) {
      expect(sql.toUpperCase()).not.toContain(forbidden);
    }
  });

  it('pins the complete {id,kind,display_name,branch_id} row image', async () => {
    const domain = await read('domain.rs');
    expect(domain).toMatch(
      /pub struct OrgNode \{[\s\S]*?pub id: String,[\s\S]*?pub kind: String,[\s\S]*?pub display_name: String,[\s\S]*?pub branch_id: Option<String>,[\s\S]*?\}/,
    );
    const postgres = await read('postgres.rs');
    expect(postgres).toContain('sqlx::query(ORG_NODES_SQL)');
    expect(postgres).toContain('.fetch_all(&mut *connection)');
    for (const field of ['id', 'kind', 'display_name', 'branch_id']) {
      expect(postgres).toContain(`row.try_get("${field}")`);
    }
  });

  it('binds Company, branch x3 and department x2 exactly as the TS tail', async () => {
    const postgres = await read('postgres.rs');
    const binds = postgres.match(/\.bind\(([^)]*)\)/g) ?? [];
    expect(binds).toHaveLength(6);
    expect(postgres).toContain('.bind(company.as_str())');
    expect(postgres.match(/\.bind\(branch\)/g)).toHaveLength(3);
    expect(postgres.match(/\.bind\(department\)/g)).toHaveLength(2);
  });

  it('owns a dedicated axum route with no SQL fallback', async () => {
    const http = await read('http.rs');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/maintenance/org-nodes');
    expect(http).toContain('get(current)');
    expect(http).toContain('x-erpx-context-company');
    expect(http).toContain('x-erpx-workspace-branch-id');
    expect(http).toContain('x-erpx-workspace-department-id');
    expect(http).not.toContain('SELECT ');
    const postgres = await read('postgres.rs');
    expect(postgres.match(/sqlx::query\(/g)).toHaveLength(1);
  });
});

describe('Maintenance Work repository org-node cutover', () => {
  it('routes the nodes read through the native transport', async () => {
    const work = await readFile(
      new URL('../../../packages/server-maintenance/src/adapters/work.ts', import.meta.url),
      'utf8',
    );
    expect(work).toContain('nativeMaintenanceOrgNodes(a)');
    expect(work).not.toContain('organization_nodes');
  });

  it('keeps the transport SQL-free and Host-explicit', async () => {
    const client = await readFile(
      new URL('../../../packages/server-maintenance/src/adapters/native-org-nodes.ts', import.meta.url),
      'utf8',
    );
    expect(client).toContain("process.env[ENV]");
    expect(client).toContain("'x-erpx-workspace-branch-id'");
    expect(client).toContain("'x-erpx-workspace-department-id'");
    expect(client).not.toContain('SELECT ');
    expect(client).not.toContain('node:http');
  });
});
