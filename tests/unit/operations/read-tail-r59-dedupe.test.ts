import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { operationsAliasSelectAll } from '../../../packages/server-operations/src/adapters/operations-alias-select-all';
import { OPERATIONS_VIEWER_CLOCK_SQL } from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r59 byte oracles captured from the retired inline
 *  `SELECT t.* FROM routine_templates t WHERE` head in routines.ts. The literal
 *  hash is fixed before the routing edit; the rendered hashes are the fully
 *  composed list/get statements (scope tail = `TRUE`). */
const retired = {
  head: 'SELECT t.* FROM routine_templates t WHERE',
  listTail: 't.company_id=? AND (t.published=1 OR t.creator_user_id=? OR ?=1) AND (TRUE) ORDER BY t.updated_at DESC,t.id',
  getTail: 't.id=? AND t.company_id=? AND (TRUE)',
  list: 'SELECT t.* FROM routine_templates t WHERE t.company_id=? AND (t.published=1 OR t.creator_user_id=? OR ?=1) AND (TRUE) ORDER BY t.updated_at DESC,t.id',
  get: 'SELECT t.* FROM routine_templates t WHERE t.id=? AND t.company_id=? AND (TRUE)',
};
const oracle = {
  head: '4e27b3d3b39cd3d7c307b4d29f3a6ded3496853b4357a480068df6b9054404ed',
  list: '4865c93ff13e55e1061a650688226c82cf35de8b5d504336d9523d57ea0a23c3',
  get: '5e7946ba0e9cbd0bac8c897b11ce7d13b2309b10a200b609de7cd05c6971f24a',
};

describe('operations read-tail r59 alias-qualified select-all head retirement', () => {
  it('renders the shared head byte-identically and stays read-only', () => {
    expect(operationsAliasSelectAll('t', 'routine_templates', retired.listTail)).toBe(retired.list);
    expect(operationsAliasSelectAll('t', 'routine_templates', retired.getTail)).toBe(retired.get);
    expect(operationsAliasSelectAll('t', 'routine_templates', '')).toBe(retired.head + ' ');
    expect(sha(retired.head)).toBe(oracle.head);
    expect(operationsAliasSelectAll('x', 'y', 'z').match(/SELECT /g)).toHaveLength(1);
    expect(binds(operationsAliasSelectAll('x', 'y', 'z'))).toBe(0);
    expect(operationsAliasSelectAll('x', 'y', 'z')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM/i);
  });

  it('renders both retired routine reads byte-identically through the oracles', () => {
    const list = operationsAliasSelectAll('t', 'routine_templates', retired.listTail);
    const get = operationsAliasSelectAll('t', 'routine_templates', retired.getTail);
    expect(list).toBe(retired.list);
    expect(get).toBe(retired.get);
    expect(sha(list)).toBe(oracle.list);
    expect(sha(get)).toBe(oracle.get);
    expect(list.startsWith(retired.head + ' ')).toBe(true);
    expect(get.startsWith(retired.head + ' ')).toBe(true);
    for (const sql of [list, get]) {
      expect(sql.match(/SELECT /g)).toHaveLength(1);
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(binds(list)).toBe(3);
    expect(binds(get)).toBe(2);
  });

  it('retires the getTemplate detail statement and its public export', async () => {
    const [routines, publicEntry] = await Promise.all([
      src('routines.ts'),
      readFile(new URL('../../../packages/server-operations/src/public/routines.ts', import.meta.url), 'utf8'),
    ]);
    expect(routines).not.toContain('getTemplate');
    expect(routines).not.toContain('readGetTemplateRow');
    expect(routines).not.toContain('operationsAliasSelectAll(');
    expect(routines).not.toContain('SELECT t.* FROM routine_templates t WHERE');
    expect(publicEntry).not.toContain('getTemplate');
    expect(publicEntry).toContain('canEditTemplate');
    expect(publicEntry).toContain('templateSchema');
  });

  it('keeps one head definition for the retained owner read sites', async () => {
    const [helper, routines] = await Promise.all([
      src('operations-alias-select-all.ts'), src('routines.ts'),
    ]);
    expect(helper.match(/export const operationsAliasSelectAll/g)).toHaveLength(1);
    expect(helper).toContain('`SELECT ${alias}.* FROM ${table} ${alias} WHERE ${tail}`');
    expect(routines).not.toContain('operationsAliasSelectAll(');
    expect(routines).not.toContain('SELECT t.* FROM routine_templates t WHERE');
    expect(routines).not.toContain('templateSelect');
  });

  it('leaves viewer clock, native reads, locks and prior r53/r55-r58 helpers untouched', async () => {
    const [scope, native, alert, locks, r53, r55, r56, r57, r58] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('native-operations-read.ts'),
      src('work-task-alert-purchases.ts'), src('project-source-locks.ts'),
      src('read-tail-r53-sources.ts'), src('read-tail-r55-sources.ts'),
      src('read-tail-r56-sources.ts'), src('read-tail-r57-sources.ts'),
      src('read-tail-r58-sources.ts'),
    ]);
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(OPERATIONS_VIEWER_CLOCK_SQL).toContain('clock_timestamp()');
    expect(native).toContain('OPERATIONS_RD_');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
    expect(r53).toContain('OPERATIONS_STAFF_TASK_HEAD');
    expect(r53).toContain('OPERATIONS_TASK_LIST_HEAD');
    expect(r55).toContain('OPERATIONS_EXISTS_HEAD');
    expect(r56).toContain('OPERATIONS_SELECT_1_FROM');
    expect(r57).toContain('OPERATIONS_OWNER_PROJECTION_ID_HEAD');
    expect(r58).toContain('OPERATIONS_PROJECTION_FROM_SOURCE');
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 65 to 64', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(58);
    expect(total).toBeLessThan(65);
  });
});
