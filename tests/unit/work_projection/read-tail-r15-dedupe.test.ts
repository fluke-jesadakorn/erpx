import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const pointCore = 'FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND t.user_id=?';

describe('work_projection read-tail r15 dedupe', () => {
  it('keeps one canonical builder for the reminder attendee point family and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const reminderAttendeePointSql =')).toHaveLength(2);

    const authority = await readFile(src('calendar/reminder-authority.ts'), 'utf8');
    expect(authority).not.toContain('SELECT ${configuration} image FROM calendar_appointments');
    expect(authority).toContain('reminderAttendeePointSql(`${configuration} image`)');

    const purchases = await readFile(src('calendar/reminder-purchases.ts'), 'utf8');
    expect(purchases).not.toContain("'SELECT a.version,a.status,a.title,a.starts_at,a.ends_at,a.timezone,t.reminder_minutes,t.response FROM calendar_appointments");
    expect(purchases).toContain("reminderAttendeePointSql('a.version,a.status,a.title,a.starts_at,a.ends_at,a.timezone,t.reminder_minutes,t.response')");
  });

  it('emits the original SQL for both parameterized callers', async () => {
    const { reminderAttendeePointSql } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    const columns = 'a.version,a.status,a.title,a.starts_at,a.ends_at,a.timezone,t.reminder_minutes,t.response';
    expect(reminderAttendeePointSql(columns)).toBe(`SELECT ${columns} ${pointCore}`);
    expect(reminderAttendeePointSql('x image')).toBe(`SELECT x image ${pointCore}`);
  });

  it('leaves the r9-r14 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    expect(await readFile(src('daily-plan.ts'), 'utf8')).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(await readFile(src('page-layouts.ts'), 'utf8')).toContain(
      'SELECT pg_advisory_xact_lock(hashtextextended(?,0))',
    );
    expect(await readFile(src('calendar/reminder-purchases.ts'), 'utf8')).toContain(
      'calendarReminderConfiguration(database,row.company_id,row.id,row.user_id)',
    );
  });
});
