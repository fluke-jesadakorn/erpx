import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  operationsChildJsonSql,
  operationsChildJsonByBoundId,
  OPERATIONS_TASK_TEMPLATE_SOURCE_SQL,
  OPERATIONS_TASK_SCHEDULE_SOURCE_SQL,
} from '../../../packages/server-operations/src/adapters/task-delegation-source';
import {
  operationsTableExists,
  operationsOwnerVersionExists,
} from '../../../packages/server-operations/src/adapters/identity-policy-version-guard';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');

describe('operations read-tail r50 closed-head retirement', () => {
  it('child-JSON helper pair renders every retired literal byte-identically', () => {
    // Exact retired literals from before the two helpers were merged.
    const template = '(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=t.template_id AND template.company_id=t.company_id)';
    const schedule = '(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=t.schedule_id AND schedule.company_id=t.company_id)';
    const boundTemplate = '(SELECT to_jsonb(t) FROM routine_templates t WHERE t.id=? AND t.company_id=?)';
    const boundSchedule = '(SELECT to_jsonb(s) FROM routine_schedules s WHERE s.id=? AND s.company_id=?)';
    expect(operationsChildJsonSql('routine_templates', 'template', 'template_id', 't')).toBe(template);
    expect(operationsChildJsonSql('routine_schedules', 'schedule', 'schedule_id', 't')).toBe(schedule);
    expect(OPERATIONS_TASK_TEMPLATE_SOURCE_SQL).toBe(template);
    expect(OPERATIONS_TASK_SCHEDULE_SOURCE_SQL).toBe(schedule);
    expect(operationsChildJsonByBoundId('routine_templates', 't')).toBe(boundTemplate);
    expect(operationsChildJsonByBoundId('routine_schedules', 's')).toBe(boundSchedule);
    // No bind changed: only the outer-row form carries a column reference.
    expect(boundTemplate).toContain('id=? AND t.company_id=?');
    expect(template).toContain('id=t.template_id AND template.company_id=t.company_id');
    for (const sql of [template, schedule, boundTemplate, boundSchedule]) {
      expect(sql.match(/SELECT /g)).toHaveLength(1);
    }
  });

  it('both exported helpers delegate to one child-JSON renderer', async () => {
    const source = await src('task-delegation-source.ts');
    expect(source.match(/\(SELECT to_jsonb\(/g)).toHaveLength(1);
    expect(source.match(/export const operationsChildJsonSql/g)).toHaveLength(1);
    expect(source.match(/export const operationsChildJsonByBoundId/g)).toHaveLength(1);
    expect(source).toContain('childJsonSql(table,alias,`${outer}.${fk}`,`${outer}.company_id`)');
    expect(source).toContain("childJsonSql(table,alias,'?','?')");
    // Neither exported helper defines its own SELECT literal any more.
    expect(source).not.toContain('export const operationsChildJsonByBoundId = (table:string,alias:string) => `(SELECT');
  });

  it('the spare iam_roles probe renders byte-identically through operationsTableExists', () => {
    const viewer = 'work_alert_viewer';
    const predicate = `role.id=${viewer}.role AND role.company_id IN ('*',t.company_id) AND role.status='active'`;
    const expected = `EXISTS(SELECT 1 FROM iam_roles role WHERE ${predicate})`;
    expect(operationsTableExists('iam_roles role', predicate)).toBe(expected);
    // Still the single canonical owner-version renderer; no new definition.
    expect(operationsTableExists('iam_roles role', predicate))
      .toBe(operationsOwnerVersionExists('iam_roles role', '', predicate));
    expect(operationsTableExists('iam_roles role', predicate).match(/SELECT /g)).toHaveLength(1);
  });

  it('retires the inline spare probe and leaves LOCK / native reads untouched', async () => {
    const [alert, guard, purchases, source] = await Promise.all([
      src('work-task-alert-purchases.ts'),
      src('identity-policy-version-guard.ts'),
      src('work-task-purchases.ts'),
      src('task-delegation-source.ts'),
    ]);
    expect(alert.match(/operationsTableExists\(/g)).toHaveLength(1);
    expect(alert).not.toContain('EXISTS(SELECT 1 FROM iam_roles role');
    expect(guard.match(/export const operationsTableExists/g)).toHaveLength(1);
    expect(guard.match(/export const operationsOwnerVersionExists/g)).toHaveLength(1);
    expect(alert).toContain('LOCK TABLE staff_tasks');
    expect(alert).toContain('OPERATIONS_RD_WORK_TASK_ALERT_IMAGE');
    expect(purchases).toContain("operationsChildJsonByBoundId('routine_templates','t')");
    expect(purchases).toContain("operationsChildJsonByBoundId('routine_schedules','s')");
    expect(source.match(/\(SELECT to_jsonb\(/g)).toHaveLength(1);
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 84 to 75', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(64);
    expect(total).toBeLessThan(84);
  });
});
