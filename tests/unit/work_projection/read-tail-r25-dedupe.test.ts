import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const joinCore =
  'FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=? AND a.company_id=?';
const original = {
  point: (columns: string) => `SELECT ${columns} ${joinCore} AND t.user_id=?`,
  recipients: (action: string) =>
    `SELECT t.user_id,a.version ${joinCore} AND a.status='scheduled' AND t.reminder_minutes IS NOT NULL${action === 'appointment.reminder.set' ? ' AND t.user_id=?' : ''}`,
} as const;

describe('work_projection read-tail r25 dedupe', () => {
  it('keeps one canonical reminder-attendee join builder and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const reminderAttendeesSql =')).toHaveLength(2);
    expect(
      shared.match(
        /`SELECT \$\{columns\} FROM calendar_appointments a JOIN calendar_attendees t ON t\.appointment_id=a\.id AND t\.company_id=a\.company_id WHERE a\.id=\? AND a\.company_id=\?\$\{tail\}`/g,
      ),
    ).toHaveLength(1);
    expect(shared).toContain('reminderAttendeesSql(columns, \' AND t.user_id=?\')');

    const authority = await readFile(src('calendar/reminder-authority.ts'), 'utf8');
    expect(authority).not.toContain(
      "SELECT t.user_id,a.version FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id",
    );
    expect(authority).toContain('reminderAttendeesSql(');
    expect(authority).toContain(
      'reminderAttendeesSql(\'t.user_id,a.version\',',
    );
  });

  it('emits the original byte-identical SQL for every parameterized caller', async () => {
    const { reminderAttendeesSql, reminderAttendeePointSql } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    for (const columns of [
      'x image',
      'a.version,a.status,a.title,a.starts_at,a.ends_at,a.timezone,t.reminder_minutes,t.response',
      't.user_id,a.version',
    ]) {
      expect(reminderAttendeePointSql(columns)).toBe(original.point(columns));
      expect(reminderAttendeesSql(columns, ' AND t.user_id=?')).toBe(original.point(columns));
    }
    for (const action of [
      'appointment.save',
      'appointment.reschedule',
      'appointment.reminder.set',
    ]) {
      expect(
        reminderAttendeesSql(
          't.user_id,a.version',
          ` AND a.status='scheduled' AND t.reminder_minutes IS NOT NULL${action === 'appointment.reminder.set' ? ' AND t.user_id=?' : ''}`,
        ),
      ).toBe(original.recipients(action));
    }
  });

  it('leaves the r9-r24 helpers, locks, binds and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'overlapProbe', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'reminderAttendeesSql', 'attendeeScopeExistsSql',
      'receiptByIdSql', 'dailyPlanSql', 'calendarImageSql',
      'appointmentAttendeeSeriesSql', 'appointmentChildImageSql', 'commandReceiptSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const authority = await readFile(src('calendar/reminder-authority.ts'), 'utf8');
    expect(authority).toContain(
      ".bind(id,actor.companyId,...(input.action==='appointment.reminder.set'?[actor.userId]:[]))",
    );
    expect(authority).toContain('calendarReminderConfiguration(');
    expect(authority).toContain('captureCalendarReminderAuthority(');

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    const layouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(layouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    const workItems = await readFile(src('work-items.ts'), 'utf8');
    expect(workItems.length).toBeGreaterThan(0);
  });

  it('retirement:ts-statements stays strictly below the r25 BEFORE=57 baseline', () => {
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
    expect(count).toBe(56);
    expect(count).toBeLessThan(57);
  });
});
