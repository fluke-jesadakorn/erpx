import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { derivedProbeSql, scopeRowSql } from '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const original = {
  actorScope: (where: string) =>
    `SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${where}`,
  release: (predicate: string) =>
    `SELECT ${predicate} allowed FROM (SELECT ?::text company_id) calendar_release`,
} as const;

describe('work_projection read-tail r30 dedupe', () => {
  it('keeps one canonical bound scope-row builder shared by the actor and release probes', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const scopeRowSql =')).toHaveLength(2);
    expect(shared.split('SELECT ?::text ${columns}')).toHaveLength(2);
    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain("derivedProbeSql(scopeRowSql('company_id,?::text branch_id,?::text department_id'),'workforce_actor_scope',where)");
    expect(purchases).toContain("scopeRowSql('company_id')");
    expect(purchases.match(/SELECT \?::text/g)).toBeNull();
  });

  it('emits the original byte-identical SQL for both parameterized callers', () => {
    const where = 'scope.company_id=? AND scope.branch_id=?';
    expect(derivedProbeSql(scopeRowSql('company_id,?::text branch_id,?::text department_id'), 'workforce_actor_scope', where))
      .toBe(original.actorScope(where));
    const predicate = 'scheduled_feature_allows(?,?)';
    expect(`SELECT ${predicate} allowed FROM (${scopeRowSql('company_id')}) calendar_release`)
      .toBe(original.release(predicate));
  });

  it('leaves the r1-r29 helpers, locks and work-item sources untouched', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionsSql', 'occurrenceExceptionPointSql',
      'appointmentAttendeesSql', 'reminderAttendeesSql', 'reminderAttendeePointSql',
      'attendeeScopeExistsSql', 'participantScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql',
      'calendarImageSql', 'appointmentAttendeeSeriesSql', 'appointmentChildImageSql',
      'commandReceiptSql', 'derivedProbeSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect((await readFile(src('daily-plan.ts'), 'utf8')))
      .toContain('SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE');
    expect((await readFile(src('page-layouts.ts'), 'utf8')))
      .toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect((await readFile(src('work-items.ts'), 'utf8')).length).toBeGreaterThan(0);
  });

  it('retirement:ts-statements stays strictly below the r29 BEFORE=52 baseline', () => {
    const count = Number(
      execFileSync(
        'git',
        ['grep', '-I', '-o', '-E', 'SELECT', '--', ':(glob)modules/work-projection/server/src/**/*.ts'],
        { encoding: 'utf8' },
      )
        .trim()
        .split('\n')
        .filter(Boolean).length,
    );
    expect(count).toBeLessThan(52);
  });
});
