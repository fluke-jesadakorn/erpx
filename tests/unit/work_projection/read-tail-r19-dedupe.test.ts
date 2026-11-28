import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const announcementImage =
  'SELECT to_jsonb(row)::text image FROM calendar_announcements row WHERE id=? AND company_id=?';
const appointmentExpression = `jsonb_build_object('record',to_jsonb(row),
 'attendees',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.user_id) FROM calendar_attendees a WHERE a.appointment_id=row.id AND a.company_id=row.company_id),
 'participants',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.person_id) FROM calendar_appointment_participants p WHERE p.appointment_id=row.id AND p.company_id=row.company_id),
 'series',(SELECT to_jsonb(s) FROM calendar_series s WHERE s.appointment_id=row.id AND s.company_id=row.company_id),
 'exceptions',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.occurrence_key) FROM calendar_occurrence_exceptions e WHERE e.appointment_id=row.id AND e.company_id=row.company_id),
 'responses',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.occurrence_key,r.user_id) FROM calendar_occurrence_responses r WHERE r.appointment_id=row.id AND r.company_id=row.company_id))::text`;
const appointmentImage = `SELECT ${appointmentExpression} image FROM calendar_appointments row WHERE id=? AND company_id=?`;

describe('work_projection read-tail r19 dedupe', () => {
  it('keeps one canonical calendar source-image builder and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const calendarImageSql =')).toHaveLength(2);

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).not.toContain("'SELECT to_jsonb(row)::text image FROM calendar_announcements");
    expect(purchases).not.toContain("`SELECT jsonb_build_object('record',to_jsonb(row)");
    expect(purchases.match(/calendarImageSql/g)?.length).toBe(3); // import + two callers
  });

  it('emits the original byte-identical SQL for both image callers', async () => {
    const { calendarImageSql } = await import(
      '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts'
    );
    expect(calendarImageSql('to_jsonb(row)::text', 'calendar_announcements')).toBe(announcementImage);
    expect(calendarImageSql(appointmentExpression, 'calendar_appointments')).toBe(appointmentImage);
  });

  it('leaves the r9-r18 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE'
    );
    expect(purchases).toContain('nativeClearCommandAssertions');
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE'
    );
    expect(daily).toContain('wp_daily_plan_read');
    expect(daily).toContain('wp_daily_plan_write');
  });
});
