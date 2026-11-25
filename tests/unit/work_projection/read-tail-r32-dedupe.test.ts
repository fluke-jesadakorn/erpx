import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { appointmentProbe, versionById } from '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  appointments: 'SELECT version FROM calendar_appointments WHERE id=? AND company_id=?',
  announcements: 'SELECT version FROM calendar_announcements WHERE id=? AND company_id=?',
  point: 'SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=?',
  pointScoped: 'SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=? AND created_by=?',
} as const;

describe('work_projection read-tail r32 dedupe', () => {
  it('keeps one canonical point-read head shared by versionById and appointmentProbe', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const versionById =')).toHaveLength(2);
    expect(shared.split('export const appointmentProbe =')).toHaveLength(2);
    expect(shared.match(/`SELECT \$\{columns\} FROM \$\{table\} WHERE id=\? AND company_id=\?\$\{extra\}`/g)).toHaveLength(1);
    expect(shared.match(/SELECT version FROM \$\{table\} WHERE id=\? AND company_id=\?/g)).toBeNull();
    expect(shared).toContain("appointmentProbe('version', '', table)");
  });

  it('emits the original byte-identical SQL for every point-read caller', () => {
    expect(versionById('calendar_appointments')).toBe(original.appointments);
    expect(versionById('calendar_announcements')).toBe(original.announcements);
    expect(appointmentProbe('version', '', 'calendar_appointments')).toBe(original.appointments);
    expect(appointmentProbe('version', '', 'calendar_announcements')).toBe(original.announcements);
    expect(appointmentProbe('1')).toBe(original.point);
    expect(appointmentProbe('1', ' AND created_by=?')).toBe(original.pointScoped);
  });

  it('leaves the r1-r31 helpers, locks and retained consumers untouched', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeesSql', 'reminderAttendeePointSql', 'attendeeScopeExistsSql',
      'participantScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql', 'calendarImageSql',
      'appointmentAttendeeSeriesSql', 'appointmentChildImageSql', 'commandReceiptSql',
      'derivedProbeSql', 'scopeRowSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(purchases).toContain('versionById(');
    expect((await readFile(src('daily-plan.ts'), 'utf8')))
      .toContain('SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE');
    expect((await readFile(src('page-layouts.ts'), 'utf8')))
      .toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect((await readFile(src('work-items.ts'), 'utf8')).length).toBeGreaterThan(0);
    expect((await readFile(src('work-cache.ts'), 'utf8')))
      .toContain("FROM work_cache_revisions WHERE table_name <> 'legal_runtime_health'");
  });

  it('retirement:ts-statements stays strictly below the r31 BEFORE=48 baseline', () => {
    const count = Number(
      execFileSync(
        'git',
        ['grep', '-I', '-o', '-E', 'SELECT', '--', ':(glob)packages/server-work_projection/src/**/*.ts'],
        { encoding: 'utf8' },
      )
        .trim()
        .split('\n')
        .filter(Boolean).length,
    );
    expect(count).toBeLessThanOrEqual(40);
    expect(count).toBeLessThan(48);
  });
});
