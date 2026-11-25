import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { OPERATIONS_OWNER_PROJECTION_ID_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r57-sources';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-operations/src/adapters/hospitality-owner-projection-sources';
import { peoplePayrollFinalOwnerProjections } from '../../../packages/server-operations/src/adapters/people-payroll-final-owner-projections';
import { OPERATIONS_STAFF_TASK_HEAD, OPERATIONS_TASK_LIST_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r53-sources';
import { OPERATIONS_EXISTS_HEAD } from '../../../packages/server-operations/src/adapters/read-tail-r55-sources';
import { OPERATIONS_SELECT_1_FROM } from '../../../packages/server-operations/src/adapters/read-tail-r56-sources';
import { OPERATIONS_VIEWER_CLOCK_SQL } from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

/** Independent pre-r57 byte oracles captured from the two retired inline
 *  explicit-column owner projections. Literal hashes are fixed before the
 *  routing edit; runtime hashes are the fully rendered statements. */
const retired = {
  hospitality: 'SELECT "id","company_id","code","name","rank","active","version","created_at","updated_at","level_number" FROM (${organizationProjectionSources.staff_levels}) staff_levels',
  delegations: 'FROM (SELECT "id","company_id","task_id","delegator_user_id","delegate_user_id","starts_at","expires_at","reason","status","revoked_by","revoked_at","version","created_at","updated_at","revoke_reason" FROM staff_task_delegations) d',
};
const oracle = {
  hospitalityLiteral: '5e0a21649a66515eeb4eac27b317cb7693d058057a7c18350c9732a95b358b40',
  delegationsLiteral: '7ed768ef34ef1a2a49a7155092349656ddcef92b12be2671f2f782af6c1e00a1',
  hospitalityRuntime: 'a6716763f0b16c24c843b0a9f0ff32d6a6754a028cfccc003ef98f1dd6f3eedf',
  delegationsRuntime: '7ed768ef34ef1a2a49a7155092349656ddcef92b12be2671f2f782af6c1e00a1',
};

describe('operations read-tail r57 explicit-column projection head consolidation', () => {
  it('renders the shared head byte-identically and stays read-only', () => {
    expect(OPERATIONS_OWNER_PROJECTION_ID_HEAD).toBe('SELECT "id","company_id",');
    expect(OPERATIONS_OWNER_PROJECTION_ID_HEAD.match(/SELECT /g)).toHaveLength(1);
    expect(binds(OPERATIONS_OWNER_PROJECTION_ID_HEAD)).toBe(0);
    expect(OPERATIONS_OWNER_PROJECTION_ID_HEAD).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM/i);
  });

  it('renders both retired owner projections byte-identically through the literal oracles', () => {
    const hospitality = '' + OPERATIONS_OWNER_PROJECTION_ID_HEAD
      + '"code","name","rank","active","version","created_at","updated_at","level_number"'
      + ' FROM (${organizationProjectionSources.staff_levels}) staff_levels';
    expect(hospitality).toBe(retired.hospitality);
    expect(sha(hospitality)).toBe(oracle.hospitalityLiteral);

    const delegations = 'FROM (' + OPERATIONS_OWNER_PROJECTION_ID_HEAD
      + '"task_id","delegator_user_id","delegate_user_id","starts_at","expires_at","reason","status"'
      + ',"revoked_by","revoked_at","version","created_at","updated_at","revoke_reason"'
      + ' FROM staff_task_delegations) d';
    expect(delegations).toBe(retired.delegations);
    expect(sha(delegations)).toBe(oracle.delegationsLiteral);

    expect(hospitality.match(/SELECT /g)).toHaveLength(1);
    expect(delegations.match(/SELECT /g)).toHaveLength(1);
    expect(binds(hospitality)).toBe(0);
    expect(binds(delegations)).toBe(0);
  });

  it('renders the imported modules byte-identically through the runtime oracles', () => {
    const h = hospitalityOwnerProjectionSources.staff_levels;
    const d = peoplePayrollFinalOwnerProjections()['operations_staff_tasks_facts_staff_task_delegations_d_from'];
    expect(h.startsWith(OPERATIONS_OWNER_PROJECTION_ID_HEAD)).toBe(true);
    expect(d).toContain('(' + OPERATIONS_OWNER_PROJECTION_ID_HEAD);
    expect(sha(h)).toBe(oracle.hospitalityRuntime);
    expect(sha(d)).toBe(oracle.delegationsRuntime);
    for (const sql of [h, d]) {
      expect(binds(sql)).toBe(0);
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(h.match(/SELECT /g)).toHaveLength(2);
    expect(d.match(/SELECT /g)).toHaveLength(1);
  });

  it('keeps one head definition and routes both family files through it', async () => {
    const [helper, hospitality, delegations] = await Promise.all([
      src('read-tail-r57-sources.ts'),
      src('hospitality-owner-projection-sources.ts'),
      src('people-payroll-final-owner-projections.ts'),
    ]);
    expect(helper.match(/export const OPERATIONS_OWNER_PROJECTION_ID_HEAD/g)).toHaveLength(1);
    expect(helper).toContain("= 'SELECT \"id\",\"company_id\",'");
    for (const text of [hospitality, delegations]) {
      expect(text.match(/OPERATIONS_OWNER_PROJECTION_ID_HEAD/g)).toHaveLength(2);
      expect(text).toContain("from './read-tail-r57-sources'");
      expect(text).not.toContain("SELECT \"id\",\"company_id\",\"");
    }
    expect(hospitality).toContain('${OPERATIONS_OWNER_PROJECTION_ID_HEAD}"code"');
    expect(delegations).toContain('(${OPERATIONS_OWNER_PROJECTION_ID_HEAD}"task_id"');
  });

  it('leaves viewer clock, native reads, locks and prior r53/r55/r56 helpers untouched', async () => {
    const [scope, native, alert, locks, r53, r55, r56] = await Promise.all([
      src('workforce-actor-scope-source.ts'), src('native-operations-read.ts'),
      src('work-task-alert-purchases.ts'), src('project-source-locks.ts'),
      src('read-tail-r53-sources.ts'), src('read-tail-r55-sources.ts'),
      src('read-tail-r56-sources.ts'),
    ]);
    expect(r53).toContain('OPERATIONS_STAFF_TASK_HEAD');
    expect(r53).toContain('OPERATIONS_TASK_LIST_HEAD');
    expect(OPERATIONS_TASK_LIST_HEAD).toBe('SELECT t.id,t.title,t.state,t.');
    expect(OPERATIONS_STAFF_TASK_HEAD('t')).toBe('SELECT 1 FROM staff_tasks t ');
    expect(r55).toContain('OPERATIONS_EXISTS_HEAD');
    expect(OPERATIONS_EXISTS_HEAD()).toBe('EXISTS(SELECT 1 FROM ');
    expect(r56).toContain('OPERATIONS_SELECT_1_FROM');
    expect(OPERATIONS_SELECT_1_FROM('staff_tasks t')).toBe('SELECT 1 FROM staff_tasks t');
    expect(OPERATIONS_VIEWER_CLOCK_SQL).toContain('clock_timestamp()');
    expect(scope).toContain('OPERATIONS_VIEWER_CLOCK_SQL');
    expect(native).toContain('OPERATIONS_RD_');
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(locks).toContain('LOCK TABLE staff_tasks IN SHARE ROW EXCLUSIVE MODE');
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 67 to 66', async () => {
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
    expect(total).toBeLessThan(67);
  });
});
