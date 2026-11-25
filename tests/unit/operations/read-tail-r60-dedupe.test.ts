import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('@erpx/server-operations/adapters/routine-source-read', () => ({
  routineSourceReadScope: async () => ({ where: 't.published=1', args: [] as unknown[] }),
}));
import { OPERATIONS_SELECT_1_FROM } from '../../../packages/server-operations/src/adapters/read-tail-r56-sources';
import { projectSourceProbe } from '../../../packages/server-operations/src/adapters/record-purchases';
import { requireWorkRoutineRead } from '../../../packages/server-operations/src/adapters/work-routine-purchases';
import { OPERATIONS_VIEWER_CLOCK_SQL } from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r60 byte oracles. The routine point-read head was inline in
 *  work-routine-purchases.ts; the routine probe is the r56 family member that
 *  already rendered the same head through OPERATIONS_SELECT_1_FROM. Hashes are
 *  fixed before the routing edit. */
const retired = {
  head: 'SELECT 1 FROM routine_templates t WHERE',
  routineRead: 'SELECT 1 FROM routine_templates t WHERE t.id=? AND t.company_id=? AND t.published=1',
  routineProbe: 'SELECT 1 FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NOT NULL AND SCOPE LIMIT 1',
};
const oracle = {
  head: 'e44d7525b2bef1f2430b66b64744421f111b55fe53d0bf2382ddf510857030f3',
  routineRead: '1ccadf6ef904673ffdd6ae294c9a5f3b2432c17242072588dcb1ed80de0828fe',
  routineProbe: '972cfe98ba3994d53ac5f6e33a28218825222ffb1489820bd3f5e418f1c76f7b',
};

describe('operations read-tail r60 routine point-read head retirement', () => {
  it('renders the shared SELECT-1-FROM head byte-identically and stays read-only', () => {
    expect(OPERATIONS_SELECT_1_FROM('routine_templates t')).toBe('SELECT 1 FROM routine_templates t');
    expect(sha(retired.head)).toBe(oracle.head);
    expect(OPERATIONS_SELECT_1_FROM('x').match(/SELECT /g)).toHaveLength(1);
    expect(binds(OPERATIONS_SELECT_1_FROM('x'))).toBe(0);
    expect(OPERATIONS_SELECT_1_FROM('x')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM/i);
  });

  it('renders both family members byte-identically through the retired oracles', () => {
    const read = OPERATIONS_SELECT_1_FROM('routine_templates t WHERE t.id=? AND t.company_id=? AND t.published=1');
    const probe = projectSourceProbe('routine_templates', 'SCOPE');
    expect(read).toBe(retired.routineRead);
    expect(sha(read)).toBe(oracle.routineRead);
    expect(probe).toBe(retired.routineProbe);
    expect(sha(probe)).toBe(oracle.routineProbe);
    for (const sql of [read, probe]) {
      expect(sql.startsWith(retired.head + ' ')).toBe(true);
      expect(sql.match(/SELECT /g)).toHaveLength(1);
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(binds(read)).toBe(2);
    expect(binds(probe)).toBe(1);
  });

  it('renders the retired SQL byte-identically through the actual owner read', async () => {
    const captured: string[] = [];
    const database = {
      prepare(sql: string) {
        captured.push(sql);
        return { bind: () => ({ first: async () => ({ id: 'tid' }) }) };
      },
    } as any;
    const actor = { companyId: 'c', branchId: 'b', departmentId: 'd', userId: 'u' } as any;
    await requireWorkRoutineRead(database, actor, 'tid');
    expect(captured).toHaveLength(1);
    expect(captured[0]).toBe(retired.routineRead);
    expect(sha(captured[0])).toBe(oracle.routineRead);
    expect(binds(captured[0])).toBe(2);
    expect(captured[0].startsWith(retired.head + ' ')).toBe(true);
  });

  it('keeps one head definition and routes both family files through it', async () => {
    const [helper, routine, purchases] = await Promise.all([
      src('read-tail-r56-sources.ts'), src('work-routine-purchases.ts'), src('record-purchases.ts'),
    ]);
    expect(helper.match(/export const OPERATIONS_SELECT_1_FROM/g)).toHaveLength(1);
    expect(helper).toContain('`SELECT 1 FROM ${source}`');
    expect(routine.match(/OPERATIONS_SELECT_1_FROM\(/g)).toHaveLength(1);
    expect(routine).toContain("from './read-tail-r56-sources'");
    expect(routine).toContain('OPERATIONS_SELECT_1_FROM(`routine_templates t WHERE t.id=?');
    expect(routine).not.toContain('`SELECT 1 FROM routine_templates t WHERE');
    expect(purchases.match(/OPERATIONS_SELECT_1_FROM\(/g)).toHaveLength(1);
    expect(routine.match(/SELECT 1 FROM/g) ?? []).toHaveLength(0);
  });

  it('leaves viewer clock, native reads, locks and prior r53-r59 helpers untouched', async () => {
    const [scope, native, alert, locks, r53, r55, r57, r58, alias] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('native-operations-read.ts'),
      src('work-task-alert-purchases.ts'), src('project-source-locks.ts'),
      src('read-tail-r53-sources.ts'), src('read-tail-r55-sources.ts'),
      src('read-tail-r57-sources.ts'), src('read-tail-r58-sources.ts'),
      src('operations-alias-select-all.ts'),
    ]);
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(OPERATIONS_VIEWER_CLOCK_SQL).toContain('clock_timestamp()');
    expect(native).toContain('OPERATIONS_RD_');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
    expect(r53).toContain('OPERATIONS_STAFF_TASK_HEAD');
    expect(r55).toContain('OPERATIONS_EXISTS_HEAD');
    expect(r57).toContain('OPERATIONS_OWNER_PROJECTION_ID_HEAD');
    expect(r58).toContain('`SELECT ${projection} FROM (${source})`');
    expect(alias).toContain('`SELECT ${alias}.* FROM ${table} ${alias} WHERE ${tail}`');
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 64 to 63', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(63);
    expect(total).toBeLessThan(64);
  });
});
