import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const norm = (sql: string) => sql.replace(/\s+/g, ' ').trim();

const original = {
  participantOverlap: "SELECT 1 FROM calendar_appointments a JOIN calendar_appointment_participants p ON p.appointment_id=a.id WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>? AND p.person_id IN (SELECT jsonb_array_elements_text(?::jsonb)) LIMIT 1",
  attendeeOverlap: `SELECT 1 FROM calendar_appointments a JOIN calendar_attendees p ON p.appointment_id=a.id
    WHERE a.company_id=? AND a.status='scheduled' AND a.id!=? AND a.starts_at<? AND a.ends_at>?
    AND p.response!='declined' AND p.user_id IN (SELECT jsonb_array_elements_text(?::jsonb)) LIMIT 1`,
  exceptionReads: {
    availability: 'SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?',
    reminder: 'SELECT cancelled,start_local FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?',
  },
} as const;

describe('work_projection read-tail r14 dedupe', () => {
  it('keeps one canonical builder per shared family and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const overlapProbe =')).toHaveLength(2);
    expect(shared.split('export const occurrenceExceptionPointSql =')).toHaveLength(2);

    const appointments = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(appointments).not.toContain("JOIN calendar_appointment_participants p ON p.appointment_id=a.id WHERE");
    expect(appointments).not.toContain("JOIN calendar_attendees p ON p.appointment_id=a.id\n");
    expect(appointments).toContain("overlapProbe('calendar_appointment_participants', 'p.person_id')");
    expect(appointments).toContain("overlapProbe('calendar_attendees', 'p.user_id', \" AND p.response!='declined'\")");

    const availability = await readFile(src('calendar-availability-facts.ts'), 'utf8');
    expect(availability).not.toContain('"SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions');
    expect(availability).toContain("occurrenceExceptionPointSql('start_local,end_local,cancelled')");

    const reminders = await readFile(src('calendar/reminder-purchases.ts'), 'utf8');
    expect(reminders).not.toContain("'SELECT cancelled,start_local FROM calendar_occurrence_exceptions");
    expect(reminders).toContain("occurrenceExceptionPointSql('cancelled,start_local')");
  });

  it('emits the original SQL for every parameterized caller', async () => {
    const { overlapProbe, occurrenceExceptionPointSql } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    expect(overlapProbe('calendar_appointment_participants', 'p.person_id')).toBe(original.participantOverlap);
    expect(norm(overlapProbe('calendar_attendees', 'p.user_id', " AND p.response!='declined'"))).toBe(
      norm(original.attendeeOverlap),
    );
    expect(overlapProbe('calendar_attendees', 'p.user_id', " AND p.response!='declined'")).toContain(
      "AND p.response!='declined' AND p.user_id IN (",
    );
    expect(occurrenceExceptionPointSql('start_local,end_local,cancelled')).toBe(
      original.exceptionReads.availability,
    );
    expect(occurrenceExceptionPointSql('cancelled,start_local')).toBe(original.exceptionReads.reminder);
  });

  it('leaves the r9-r13 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql', 'workCacheRevisionsSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    expect(await readFile(src('daily-plan.ts'), 'utf8')).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(await readFile(src('page-layouts.ts'), 'utf8')).toContain(
      'SELECT pg_advisory_xact_lock(hashtextextended(?,0))',
    );
    const remindersFacts = await readFile(src('calendar-reminders-facts.ts'), 'utf8');
    expect(remindersFacts).toContain('readOccurrenceException(db,facts)');
    expect(remindersFacts).toContain('readOccurrenceResponse(db,facts)');
  });
});
