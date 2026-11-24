import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import {
  appointmentAttendeesSql,
  appointmentAttendeeSeriesSql,
  reminderAttendeesSql,
  reminderAttendeePointSql,
} from '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const head = 'SELECT ${columns} FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id';

const original = {
  reminder: 'SELECT t.user_id,a.version FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status=\'scheduled\' AND t.reminder_minutes IS NOT NULL',
  point: 'SELECT 1 FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND t.user_id=?',
  series: 'SELECT a.id FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id LEFT JOIN calendar_series s ON s.appointment_id=a.id WHERE a.company_id=?',
} as const;

describe('work_projection read-tail r29 dedupe', () => {
  it('keeps one canonical appointment/attendee join head and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const appointmentAttendeesSql =')).toHaveLength(2);
    expect(shared.split(head).length - 1).toBe(1);
    expect(shared).toContain('appointmentAttendeesSql(columns, `WHERE a.id=? AND a.company_id=?${tail}`)');
    expect(shared).toContain('appointmentAttendeesSql(columns, `LEFT JOIN calendar_series s ON s.appointment_id=a.id WHERE ${where}`)');
  });

  it('emits the original byte-identical SQL for every parameterized caller', () => {
    expect(appointmentAttendeesSql('t.user_id,a.version', 'WHERE a.id=? AND a.company_id=? AND a.status=\'scheduled\' AND t.reminder_minutes IS NOT NULL')).toBe(original.reminder);
    expect(reminderAttendeesSql('t.user_id,a.version', ' AND a.status=\'scheduled\' AND t.reminder_minutes IS NOT NULL')).toBe(original.reminder);
    expect(reminderAttendeePointSql('1')).toBe(original.point);
    expect(appointmentAttendeeSeriesSql('a.id', 'a.company_id=?')).toBe(original.series);
  });

  it('leaves the r1-r28 helpers, locks and work-item sources untouched', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'overlapProbe', 'occurrenceExceptionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeesSql', 'reminderAttendeePointSql', 'attendeeScopeExistsSql',
      'participantScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql', 'calendarImageSql',
      'appointmentAttendeeSeriesSql', 'appointmentChildImageSql', 'commandReceiptSql',
      'derivedProbeSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases.match(/SELECT 1 FROM \(SELECT \?::text company_id,\?::text branch_id,\?::text department_id\) workforce_actor_scope WHERE \$\{where\}/g)).toHaveLength(1);
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain('SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE');
    const layouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(layouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect((await readFile(src('work-items.ts'), 'utf8')).length).toBeGreaterThan(0);
  });

  it('retirement:ts-statements stays strictly below the r28 BEFORE=53 baseline', () => {
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
    expect(count).toBeLessThanOrEqual(52);
    expect(count).toBeLessThan(53);
  });
});
