import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  series: 'SELECT * FROM calendar_series WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
  exceptions: 'SELECT * FROM calendar_occurrence_exceptions WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
  responses: 'SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
} as const;

describe('work_projection read-tail r23 dedupe', () => {
  it('keeps one canonical byAppointmentIds batch builder and no inline responses copy', async () => {
    const facts = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(facts.match(/`SELECT \$\{columns\} FROM \$\{table\} WHERE company_id=\? AND \$\{inList\("appointment_id"\)\}`/g)).toHaveLength(1);
    expect(facts).not.toContain('SELECT appointment_id,occurrence_key,user_id,response FROM calendar_occurrence_responses WHERE');
    expect(facts).toContain("byAppointmentIds('calendar_occurrence_responses', 'appointment_id,occurrence_key,user_id,response')");
  });

  it('emits the original byte-identical SQL for every parameterized caller', async () => {
    const { byAppointmentIds } = await import(
      '../../../packages/server-work_projection/src/adapters/calendar-appointments-facts.ts'
    );
    expect(byAppointmentIds('calendar_series')).toBe(original.series);
    expect(byAppointmentIds('calendar_occurrence_exceptions')).toBe(original.exceptions);
    expect(byAppointmentIds('calendar_occurrence_responses', 'appointment_id,occurrence_key,user_id,response')).toBe(original.responses);
  });

  it('leaves the r9-r22 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'overlapProbe', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql',
      'calendarImageSql', 'appointmentAttendeeSeriesSql', 'appointmentChildImageSql',
      'commandReceiptSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const facts = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(facts).toContain("byAppointmentIds('calendar_series')");
    expect(facts).toContain("byAppointmentIds('calendar_occurrence_exceptions')");
    expect(facts).toContain("overlapProbe('calendar_appointment_participants', 'p.person_id')");
    expect(facts).toContain("overlapProbe('calendar_attendees', 'p.user_id', \" AND p.response!='declined'\")");
    expect(facts).toContain('attendeeScopeExistsSql');

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(purchases).toContain('commandReceiptSql(');
    expect(purchases).toContain('calendarManualSourceReadLocks');
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain('SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE');
    const pageLayouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(pageLayouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
  });

  it('retirement:ts-statements stays strictly below the r23 BEFORE=59 baseline', () => {
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
    expect(count).toBeLessThan(59);
  });
});
