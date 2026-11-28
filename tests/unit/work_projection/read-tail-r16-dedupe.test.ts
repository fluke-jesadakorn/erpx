import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const scopeArm =
  'EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?)';

describe('work_projection read-tail r16 dedupe', () => {
  it('keeps one canonical builder for the attendee scope arm and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const attendeeScopeExistsSql =')).toHaveLength(2);

    const facts = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(facts).not.toContain(scopeArm);
    expect(facts).not.toContain('attendeeScopeExistsSql');
    expect(facts).toContain('nativeAppointmentCandidates(');
  });

  it('emits the original byte-identical scope arm for every interpolated caller', async () => {
    const { attendeeScopeExistsSql } = await import(
      '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts'
    );
    expect(attendeeScopeExistsSql).toBe(scopeArm);
  });

  it('leaves the r9-r15 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    expect(await readFile(src('daily-plan.ts'), 'utf8')).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(await readFile(src('page-layouts.ts'), 'utf8')).toContain(
      'SELECT pg_advisory_xact_lock(hashtextextended(?,0))',
    );
    expect(await readFile(src('calendar/reminder-authority.ts'), 'utf8')).toContain(
      'calendarReminderConfiguration(database,actor.companyId,id,recipient.user_id)',
    );
  });
});
