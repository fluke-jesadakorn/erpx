import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { OPERATIONS_PROJECTION_FROM_SOURCE } from '../../../packages/server-operations/src/adapters/read-tail-r58-sources';
import { operationsActiveStaffLevels } from '../../../packages/server-operations/src/adapters/staff-facts';
import { operationsOverdueAlertSql } from '../../../packages/server-operations/src/adapters/executive-task-cohort';
import { finalRootOwnerProjectionSources as organizationProjectionSources } from '@erpx/server-organization/public/final-root-owner-projection-sources';
import { finalRootOwnerProjectionSources as collaborationProjectionSources } from '@erpx/server-collaboration/public/final-root-owner-projection-sources';
import { OPERATIONS_VIEWER_CLOCK_SQL } from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';
import { OPERATIONS_STAFF_TASK_HEAD, OPERATIONS_TASK_LIST_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r53-sources';
import { OPERATIONS_EXISTS_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r55-sources';
import { OPERATIONS_SELECT_1_FROM } from '../../../packages/server-operations/src/adapters/read-tail-r56-sources';
import { OPERATIONS_OWNER_PROJECTION_ID_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r57-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r58 byte oracles captured from the two retired inline
 *  `SELECT ${projection} FROM (${source})` heads. Literal hashes are fixed
 *  before the routing edit; rendered hashes are the fully composed statements. */
const retired = {
  staff: `SELECT id,rank FROM (${organizationProjectionSources.staff_levels}) staff_levels WHERE company_id=? AND active=1`,
  overdue: `SELECT 1 FROM (${collaborationProjectionSources.staff_notifications}) n WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert' AND n.record_id=t.id AND n.record_version=t.version AND n.id LIKE 'staff-task-alert-overdue-%'`,
  overdueSep: `SELECT 1 FROM (${collaborationProjectionSources.staff_notifications}) n\n        WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert'\n          AND n.record_id=t.id AND n.record_version=t.version\n          AND n.id LIKE 'staff-task-alert-overdue-%'`,
};
const oracle = {
  staffHead: '77c5efb973fac990b29b600e433a38437be545686b37f27d5789eac7125c54f9',
  overdueHead: '0d69ea1c928405751c599b96582660d07be5516fb1a878f3ef4f8b618d0573a9',
  staff: '14831355462ef2164edc5a22ea4b4b424eff791e70a49fb73e286b3a3f118fd7',
  overdue: 'bab603b35a63be4d7101ffbfbac45287debd6892f5bca7882dab046c5d69db79',
  overdueSep: 'cf621cddcabc9d1cd3a5d9bf19b0473af76e37700a264b3ec158de6d98bc5d6e',
};

describe('operations read-tail r58 projection-source head consolidation', () => {
  it('renders the shared head byte-identically and stays read-only', () => {
    expect(OPERATIONS_PROJECTION_FROM_SOURCE('x', 'y')).toBe('SELECT x FROM (y)');
    expect(OPERATIONS_PROJECTION_FROM_SOURCE('x', 'y').match(/SELECT /g)).toHaveLength(1);
    expect(binds(OPERATIONS_PROJECTION_FROM_SOURCE('x', 'y'))).toBe(0);
    expect(OPERATIONS_PROJECTION_FROM_SOURCE('x', 'y')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM/i);
    const staffHead = OPERATIONS_PROJECTION_FROM_SOURCE('id,rank', organizationProjectionSources.staff_levels);
    const overdueHead = OPERATIONS_PROJECTION_FROM_SOURCE('1', collaborationProjectionSources.staff_notifications);
    expect(staffHead).toBe(`SELECT id,rank FROM (${organizationProjectionSources.staff_levels})`);
    expect(overdueHead).toBe(`SELECT 1 FROM (${collaborationProjectionSources.staff_notifications})`);
    expect(sha(staffHead)).toBe(oracle.staffHead);
    expect(sha(overdueHead)).toBe(oracle.overdueHead);
  });

  it('renders both retired owner projections byte-identically through the oracles', () => {
    const staff = operationsActiveStaffLevels('id,rank');
    expect(staff).toBe(retired.staff);
    expect(sha(staff)).toBe(oracle.staff);
    expect(operationsActiveStaffLevels('id,rank', ' ORDER BY rank')).toBe(retired.staff + ' ORDER BY rank');

    const overdue = operationsOverdueAlertSql('1', 't');
    expect(overdue).toBe(retired.overdue);
    expect(sha(overdue)).toBe(oracle.overdue);
    const overdueSep = operationsOverdueAlertSql('1', 't', '\n        ', '\n          ');
    expect(overdueSep).toBe(retired.overdueSep);
    expect(sha(overdueSep)).toBe(oracle.overdueSep);

    for (const sql of [staff, overdue, overdueSep]) {
      expect(binds(sql)).toBe(1);
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM/i);
    }
    // The embedded native projection contributes its own SELECT; the head adds exactly one.
    expect(staff.match(/SELECT /g)).toHaveLength(2);
    expect(overdue.match(/SELECT /g)).toHaveLength(2);
  });

  it('keeps one head definition and routes both family files through it', async () => {
    const [helper, staff, cohort] = await Promise.all([
      src('read-tail-r58-sources.ts'), src('staff-facts.ts'), src('executive-task-cohort.ts'),
    ]);
    expect(helper.match(/export const OPERATIONS_PROJECTION_FROM_SOURCE/g)).toHaveLength(1);
    expect(helper).toContain('`SELECT ${projection} FROM (${source})`');
    for (const text of [staff, cohort]) {
      expect(text.match(/OPERATIONS_PROJECTION_FROM_SOURCE\(/g)).toHaveLength(1);
      expect(text).toContain("from './read-tail-r58-sources'");
      expect(text).not.toContain('SELECT ${projection} FROM (');
    }
    expect(staff).toContain('${OPERATIONS_PROJECTION_FROM_SOURCE(projection, organizationProjectionSources.staff_levels)} staff_levels WHERE');
    expect(cohort).toContain('${OPERATIONS_PROJECTION_FROM_SOURCE(projection, collaborationProjectionSources.staff_notifications)} n${whereSep}WHERE');
  });

  it('leaves viewer clock, native reads, locks and prior r53/r55/r56/r57 helpers untouched', async () => {
    const [scope, native, alert, locks, r53, r55, r56, r57] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('native-operations-read.ts'),
      src('work-task-alert-purchases.ts'), src('project-source-locks.ts'),
      src('read-tail-r53-sources.ts'), src('read-tail-r55-sources.ts'),
      src('read-tail-r56-sources.ts'), src('read-tail-r57-sources.ts'),
    ]);
    expect(OPERATIONS_VIEWER_CLOCK_SQL).toContain('clock_timestamp()');
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(r53).toContain('OPERATIONS_STAFF_TASK_HEAD');
    expect(OPERATIONS_TASK_LIST_HEAD).toBe('SELECT t.id,t.title,t.state,t.');
    expect(OPERATIONS_STAFF_TASK_HEAD('t')).toBe('SELECT 1 FROM staff_tasks t ');
    expect(r55).toContain('OPERATIONS_EXISTS_HEAD');
    expect(OPERATIONS_EXISTS_HEAD()).toBe('EXISTS(SELECT 1 FROM ');
    expect(r56).toContain('OPERATIONS_SELECT_1_FROM');
    expect(OPERATIONS_SELECT_1_FROM('staff_tasks t')).toBe('SELECT 1 FROM staff_tasks t');
    expect(r57).toContain('OPERATIONS_OWNER_PROJECTION_ID_HEAD');
    expect(OPERATIONS_OWNER_PROJECTION_ID_HEAD).toBe('SELECT "id","company_id",');
    expect(native).toContain('OPERATIONS_RD_');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 66 to 65', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(65);
    expect(total).toBeLessThan(66);
  });
});
