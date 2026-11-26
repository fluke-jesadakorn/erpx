import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

import { PP_RD18_HR_PEOPLE_PROJ_SQL } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r18-sources';
import { assignmentsProjectionCompact } from '../../../packages/server-identity_access/src/adapters/assignment-projection-source';

const slice = new URL(
  '../../../packages/server-maintenance/src/adapters/native_owner_options/',
  import.meta.url,
);
const read = (name: string) => readFile(new URL(name, slice), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

/** The exact statement the TypeScript `people` repository branch composes from
 * the two closed owner projections. The native slice must execute this literal
 * with a single `$1` Company bind, not a plan or a TypeScript-prepared query. */
const oracle =
  'SELECT DISTINCT p.user_id id,p.legal_name name ' +
  `FROM (${PP_RD18_HR_PEOPLE_PROJ_SQL}) p ` +
  `JOIN (${assignmentsProjectionCompact}) x ` +
  'ON x.user_id=p.user_id AND x.company_id=p.company_id ' +
  "WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL " +
  'AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ' +
  "AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name";

async function rustSql(): Promise<string> {
  const text = await read('domain.rs');
  const match = text.match(/OWNER_OPTIONS_SQL: &str = "([\s\S]*?)";/);
  if (!match) throw new Error('OWNER_OPTIONS_SQL constant not found');
  return match[1]
    .split('\\\n')
    .map((line) => line.trim())
    .join(' ')
    .replace(/\\"/g, '"');
}

describe('native Maintenance owner-option read parity', () => {
  it('executes the same composed statement as the TypeScript authority', async () => {
    const sql = await rustSql();
    expect(sql).toBe(oracle);
    expect(sha(sql)).toBe(sha(oracle));
    expect(sql.match(/\$1/g)).toHaveLength(1);
    expect(sql).not.toContain('?');
    for (const forbidden of ['INSERT', 'UPDATE ', 'DELETE ', 'LOCK ']) {
      expect(sql.toUpperCase()).not.toContain(forbidden);
    }
  });

  it('pins the complete {id,name} row image used by the preventive console', async () => {
    const domain = await read('domain.rs');
    expect(domain).toMatch(/pub struct OwnerOption \{[\s\S]*?pub id: String,[\s\S]*?pub name: String,[\s\S]*?\}/);
    expect(domain).not.toMatch(/pub struct OwnerOption \{[\s\S]*?pub [a-z_]+: (?!String)/);
    const postgres = await read('postgres.rs');
    expect(postgres).toContain('sqlx::query(OWNER_OPTIONS_SQL)');
    expect(postgres).toContain('.fetch_all(&mut *connection)');
    expect(postgres).toContain('row.try_get("id")');
    expect(postgres).toContain('row.try_get("name")');
  });

  it('owns a dedicated axum route with no SQL fallback', async () => {
    const http = await read('http.rs');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/maintenance/owner-options');
    expect(http).toContain('get(current)');
    expect(http).toContain('x-erpx-context-company');
    expect(http).not.toContain('SELECT ');
    const mod = await read('postgres.rs');
    expect(mod.match(/sqlx::query\(/g)).toHaveLength(1);
  });
});
