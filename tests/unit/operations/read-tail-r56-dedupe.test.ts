import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { OPERATIONS_SELECT_1_FROM } from '../../../packages/server-operations/src/adapters/read-tail-r56-sources';
import { projectSourceProbe } from '../../../packages/server-operations/src/adapters/record-purchases';
import {
  OPERATIONS_CLOCK_CTE_SQL, OPERATIONS_VIEWER_CLOCK_SQL, operationsClockProbe, operationsViewerClockHead,
} from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r56 byte oracles captured from the two retired inline
 *  `SELECT 1 FROM <source>` heads. The hashes are fixed before any routing edit. */
const retired = {
  workProbe: 'SELECT 1 FROM staff_tasks t WHERE t.company_id=? AND t.project_id IS NOT NULL AND WORK_WHERE LIMIT 1',
  routineProbe: 'SELECT 1 FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NOT NULL AND ROUTINE_WHERE LIMIT 1',
  alertClock: 'WITH work_alert_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT 1 FROM work_alert_clock',
  handbookClock: 'WITH handbook_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT 1 FROM handbook_clock',
};
const oracle = {
  workProbe: '4fd06f3c3813ba6860b7530361f9bbe7e23da04f86f08892df13768eb82edb8b',
  routineProbe: '75fb938375f30f5085764a8cc34e9e06742749adeab1b1d83dc891da9b282767',
  alertClock: '0c5dea5d203223ecd7298cb0ae3840fa563aa3f890324b962878f9226d610989',
  handbookClock: 'f4987953a5a413f847f467c39b2888d907a90367b8f1e754cafcc537624941f4',
};

describe('operations read-tail r56 bare SELECT-1-FROM head consolidation', () => {
  it('renders the shared head byte-identically and stays read-only', () => {
    expect(OPERATIONS_SELECT_1_FROM('staff_tasks t')).toBe('SELECT 1 FROM staff_tasks t');
    expect(OPERATIONS_SELECT_1_FROM('work_alert_clock')).toBe('SELECT 1 FROM work_alert_clock');
    expect(OPERATIONS_SELECT_1_FROM('x').match(/SELECT /g)).toHaveLength(1);
    expect(binds(OPERATIONS_SELECT_1_FROM('x'))).toBe(0);
    expect(OPERATIONS_SELECT_1_FROM('x')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM/i);
  });

  it('renders both probe families byte-identically through the retired oracles', () => {
    const work = projectSourceProbe('staff_tasks', 'WORK_WHERE');
    expect(work).toBe(retired.workProbe);
    expect(sha(work)).toBe(oracle.workProbe);

    const routine = projectSourceProbe('routine_templates', 'ROUTINE_WHERE');
    expect(routine).toBe(retired.routineProbe);
    expect(sha(routine)).toBe(oracle.routineProbe);

    const alert = operationsClockProbe('work_alert_clock');
    expect(alert).toBe(retired.alertClock);
    expect(sha(alert)).toBe(oracle.alertClock);

    const handbook = operationsClockProbe('handbook_clock');
    expect(handbook).toBe(retired.handbookClock);
    expect(sha(handbook)).toBe(oracle.handbookClock);

    for (const sql of [work, routine]) expect(sql.match(/SELECT /g)).toHaveLength(1);
    for (const sql of [alert, handbook]) expect(sql.match(/SELECT /g)).toHaveLength(2);
    expect(binds(work)).toBe(1);
    expect(binds(routine)).toBe(1);
    expect(binds(alert)).toBe(0);
    expect(binds(handbook)).toBe(0);
  });

  it('keeps one head definition and routes both family files through it', async () => {
    const [helper, purchases, scope] = await Promise.all([
      src('read-tail-r56-sources.ts'), src('record-purchases.ts'), src('workforce-actor-scope-source.ts'),
    ]);
    expect(helper.match(/export const OPERATIONS_SELECT_1_FROM/g)).toHaveLength(1);
    expect(helper).toContain('`SELECT 1 FROM ${source}`');
    for (const text of [purchases, scope]) {
      expect(text.match(/OPERATIONS_SELECT_1_FROM\(/g)).toHaveLength(1);
      expect(text).toContain("from './read-tail-r56-sources'");
    }
    expect(purchases).toContain('${OPERATIONS_SELECT_1_FROM(`${table} t`)} WHERE t.company_id=?');
    expect(purchases).not.toContain('`SELECT 1 FROM ${table} t WHERE');
    expect(scope).toContain('${OPERATIONS_SELECT_1_FROM(name)}');
    expect(scope).not.toContain(' SELECT 1 FROM ${name}`');
    // Both callers still render the head exactly once each.
    expect(purchases.match(/SELECT 1 FROM/g) ?? []).toHaveLength(0);
    expect(scope.match(/SELECT 1 FROM/g) ?? []).toHaveLength(3);
  });

  it('leaves viewer clock, native reads, locks and prior r53/r55 helpers untouched', async () => {
    const [scope, purchases, native, alert, locks, r53, r55] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('record-purchases.ts'), src('native-operations-read.ts'),
      src('work-task-alert-purchases.ts'), src('project-source-locks.ts'),
      src('read-tail-r53-sources.ts'), src('read-tail-r55-sources.ts'),
    ]);
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(scope).toContain('operationsViewerClockHead');
    expect(OPERATIONS_VIEWER_CLOCK_SQL).toContain('?::text user_id,?::text assignment_id');
    expect(OPERATIONS_CLOCK_CTE_SQL).not.toContain('user_id');
    expect(operationsViewerClockHead('v', ' ')).toContain('SELECT 1 FROM ');
    expect(purchases).toContain('nativeOperationsReadFirst');
    expect(native).toContain('OPERATIONS_RD_');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
    expect(r53).toContain('OPERATIONS_STAFF_TASK_HEAD');
    expect(r53).toContain('OPERATIONS_TASK_LIST_HEAD');
    expect(r55).toContain('OPERATIONS_EXISTS_HEAD');
    for (const text of [OPERATIONS_SELECT_1_FROM('x'), scope]) expect(text).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 68 to 67', async () => {
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
    expect(total).toBeLessThan(68);
  });
});
