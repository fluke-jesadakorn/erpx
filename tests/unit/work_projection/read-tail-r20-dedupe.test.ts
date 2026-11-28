import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const norm = (sql: string) => sql.replace(/\s*,\s*/g, ',').replace(/\s+/g, ' ').trim();

const availabilityColumns =
  'a.id,a.starts_at,a.ends_at,a.timezone,a.status,a.all_day,' +
  's.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,t.user_id,t.response';
const availabilityWhere =
  "a.company_id=? AND a.status='scheduled' AND a.starts_at<? AND t.response!='declined' AND t.user_id IN (?,?,?)";

const reminderColumns =
  'a.id,a.company_id,a.title,a.version,a.starts_at,a.ends_at,a.timezone,' +
  's.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,' +
  't.user_id,t.reminder_minutes,t.response';
const reminderWhere =
  "a.status='scheduled' AND a.starts_at<? AND t.reminder_minutes IS NOT NULL AND t.response!='declined' " +
  "AND (s.appointment_id IS NOT NULL OR a.ends_at>?) AND (a.id||':'||t.user_id)>? AND TRUE " +
  'ORDER BY a.id,t.user_id LIMIT 200';

const original = {
  availability: `SELECT a.id,a.starts_at,a.ends_at,a.timezone,a.status,a.all_day,
    s.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,t.user_id,t.response
    FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id
    LEFT JOIN calendar_series s ON s.appointment_id=a.id
    WHERE a.company_id=? AND a.status='scheduled' AND a.starts_at<? AND t.response!='declined'
    AND t.user_id IN (?,?,?)`,
  reminder: `SELECT a.id,a.company_id,a.title,a.version,a.starts_at,a.ends_at,a.timezone,
      s.frequency,s.interval_count,s.weekdays,s.end_kind,s.occurrence_count,s.until_date,
      t.user_id,t.reminder_minutes,t.response FROM calendar_appointments a
      JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id
      LEFT JOIN calendar_series s ON s.appointment_id=a.id
      WHERE a.status='scheduled' AND a.starts_at<? AND t.reminder_minutes IS NOT NULL AND t.response!='declined'
      AND (s.appointment_id IS NOT NULL OR a.ends_at>?) AND (a.id||':'||t.user_id)>?
      AND TRUE
      ORDER BY a.id,t.user_id LIMIT 200`,
} as const;

describe('work_projection read-tail r20 dedupe', () => {
  it('keeps one canonical appointment/attendee/series builder and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const appointmentAttendeeSeriesSql =')).toHaveLength(2);

    const inlineJoin =
      'FROM calendar_appointments a JOIN calendar_attendees t ON t.appointment_id=a.id AND t.company_id=a.company_id LEFT JOIN calendar_series s ON s.appointment_id=a.id';
    const availability = await readFile(src('calendar-availability-facts.ts'), 'utf8');
    const reminders = await readFile(src('calendar-reminders-facts.ts'), 'utf8');
    expect(norm(availability)).not.toContain(norm(inlineJoin));
    expect(norm(reminders)).not.toContain(norm(inlineJoin));
    // Lane 29 cutover: the availability series read delegates to the native client.
    expect(availability).toContain('nativeAvailabilitySources(');
    expect(reminders).toContain('appointmentAttendeeSeriesSql(');
  });

  it('emits the original SQL for both parameterized callers', async () => {
    const { appointmentAttendeeSeriesSql } = await import(
      '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts'
    );
    expect(norm(appointmentAttendeeSeriesSql(availabilityColumns, availabilityWhere))).toBe(
      norm(original.availability),
    );
    expect(norm(appointmentAttendeeSeriesSql(reminderColumns, reminderWhere))).toBe(
      norm(original.reminder),
    );
  });

  it('leaves the r9-r19 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql',
      'calendarImageSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    // Locks and retention contracts are unchanged.
    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(purchases).toContain('nativeClearCommandAssertions');
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(daily).toContain('wp_daily_plan_read');
    expect(daily).toContain('wp_daily_plan_write');
    const pageLayouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(pageLayouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');

    // Reminder fact units keep their shared occurrence readers and native writers.
    const reminders = await readFile(src('calendar-reminders-facts.ts'), 'utf8');
    expect(reminders).toContain('readOccurrenceException(db,facts)');
    expect(reminders).toContain('readOccurrenceResponse(facts)');
    expect(reminders).toContain('nativeOwnerWriteRow');
  });
});
