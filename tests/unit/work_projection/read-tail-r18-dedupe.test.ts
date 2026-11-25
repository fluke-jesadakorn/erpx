import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const lock =
  'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE';
const version =
  'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=?';
const written =
  'SELECT 1 FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? AND version=? AND last_command_id=?';

describe('work_projection read-tail r18 dedupe', () => {
  it('keeps one canonical staff_daily_plans composite-key builder and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const dailyPlanSql =')).toHaveLength(2);

    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).not.toContain('COALESCE((SELECT version FROM staff_daily_plans');
    expect(daily).not.toContain('EXISTS(SELECT 1 FROM staff_daily_plans');
    expect(daily.split('dailyPlanSql(')).toHaveLength(3); // import + two assertion callers
  });

  it('emits the original byte-identical SQL for both assertion callers', async () => {
    const { dailyPlanSql } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    expect(dailyPlanSql('version')).toBe(version);
    expect(dailyPlanSql('1', ' AND version=? AND last_command_id=?')).toBe(written);
  });

  it('leaves the r9-r17 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql', 'receiptByIdSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain(lock);
    expect(daily).toContain('wp_daily_plan_read');
    expect(daily).toContain('wp_daily_plan_write');
  });
});
