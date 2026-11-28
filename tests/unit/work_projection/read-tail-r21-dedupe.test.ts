import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const norm = (sql: string) => sql.replace(/\s*,\s*/g, ',').replace(/\s+/g, ' ').trim();

const original = {
  attendees:
    '(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.user_id) FROM calendar_attendees a WHERE a.appointment_id=row.id AND a.company_id=row.company_id)',
  participants:
    '(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.person_id) FROM calendar_appointment_participants p WHERE p.appointment_id=row.id AND p.company_id=row.company_id)',
  series:
    '(SELECT to_jsonb(s) FROM calendar_series s WHERE s.appointment_id=row.id AND s.company_id=row.company_id)',
  exceptions:
    '(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.occurrence_key) FROM calendar_occurrence_exceptions e WHERE e.appointment_id=row.id AND e.company_id=row.company_id)',
  responses:
    '(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.occurrence_key,r.user_id) FROM calendar_occurrence_responses r WHERE r.appointment_id=row.id AND r.company_id=row.company_id)',
  reminderSeries:
    "(SELECT to_jsonb(s)-'version' FROM calendar_series s WHERE s.appointment_id=a.id AND s.company_id=a.company_id)",
  reminderExceptions:
    "(SELECT jsonb_agg(to_jsonb(e)-'version' ORDER BY e.occurrence_key) FROM calendar_occurrence_exceptions e WHERE e.appointment_id=a.id AND e.company_id=a.company_id)",
} as const;

describe('work_projection read-tail r21 dedupe', () => {
  it('keeps one canonical appointment-keyed child image builder and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const appointmentChildImageSql =')).toHaveLength(2);

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    const reminders = await readFile(src('calendar/reminder-authority.ts'), 'utf8');
    expect(purchases).toContain('appointmentChildImageSql(');
    expect(reminders).toContain('appointmentChildImageSql(');
    for (const inline of [
      'FROM calendar_attendees a WHERE a.appointment_id=row.id',
      'FROM calendar_appointment_participants p WHERE p.appointment_id=row.id',
      'FROM calendar_series s WHERE s.appointment_id=row.id',
      'FROM calendar_occurrence_exceptions e WHERE e.appointment_id=row.id',
      'FROM calendar_occurrence_responses r WHERE r.appointment_id=row.id',
    ]) {
      expect(norm(purchases)).not.toContain(norm(inline));
    }
    for (const inline of [
      'FROM calendar_series s WHERE s.appointment_id=a.id',
      'FROM calendar_occurrence_exceptions e WHERE e.appointment_id=a.id',
    ]) {
      expect(norm(reminders)).not.toContain(norm(inline));
    }
  });

  it('emits the original SQL for every parameterized caller', async () => {
    const { appointmentChildImageSql } = await import(
      '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts'
    );
    const p = appointmentChildImageSql;
    expect(
      p('jsonb_agg(to_jsonb(a) ORDER BY a.user_id)', 'calendar_attendees', 'a', 'row'),
    ).toBe(original.attendees);
    expect(
      p('jsonb_agg(to_jsonb(p) ORDER BY p.person_id)', 'calendar_appointment_participants', 'p', 'row'),
    ).toBe(original.participants);
    expect(p('to_jsonb(s)', 'calendar_series', 's', 'row')).toBe(original.series);
    expect(
      p('jsonb_agg(to_jsonb(e) ORDER BY e.occurrence_key)', 'calendar_occurrence_exceptions', 'e', 'row'),
    ).toBe(original.exceptions);
    expect(
      p('jsonb_agg(to_jsonb(r) ORDER BY r.occurrence_key,r.user_id)', 'calendar_occurrence_responses', 'r', 'row'),
    ).toBe(original.responses);
    expect(p("to_jsonb(s)-'version'", 'calendar_series', 's', 'a')).toBe(original.reminderSeries);
    expect(
      p("jsonb_agg(to_jsonb(e)-'version' ORDER BY e.occurrence_key)", 'calendar_occurrence_exceptions', 'e', 'a'),
    ).toBe(original.reminderExceptions);
  });

  it('leaves the r9-r20 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql',
      'calendarImageSql', 'appointmentAttendeeSeriesSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(purchases).toContain('nativeClearCommandAssertions');
    expect(purchases).toContain('calendarImageSql(');
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(daily).toContain('wp_daily_plan_read');
    expect(daily).toContain('wp_daily_plan_write');
    const pageLayouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(pageLayouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    const reminders = await readFile(src('calendar-reminders-facts.ts'), 'utf8');
    expect(reminders).toContain('readOccurrenceException(db,facts)');
    expect(reminders).toContain('readOccurrenceResponse(facts)');
    expect(reminders).toContain('nativeOwnerWriteRow');
  });
});
