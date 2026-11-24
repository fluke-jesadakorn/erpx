import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import {
  occurrenceExceptionPointSql,
  occurrenceExceptionsSql,
} from '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  availabilityPoint: 'SELECT start_local,end_local,cancelled FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?',
  reminderPoint: 'SELECT cancelled,start_local FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?',
  movedOccurrences: 'SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL',
} as const;

describe('work_projection read-tail r28 dedupe', () => {
  it('keeps one canonical occurrence-exception SELECT head and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const occurrenceExceptionsSql =')).toHaveLength(2);
    expect(shared.match(/`SELECT \$\{columns\} FROM calendar_occurrence_exceptions WHERE appointment_id=\? AND company_id=\? AND \$\{tail\}`/g)).toHaveLength(1);
    expect(shared).toContain("occurrenceExceptionsSql(columns, 'occurrence_key=?')");
    expect(shared).not.toContain('`SELECT ${columns} FROM calendar_occurrence_exceptions WHERE appointment_id=? AND company_id=? AND occurrence_key=?`');

    const availability = await readFile(src('calendar-availability-facts.ts'), 'utf8');
    expect(availability).not.toContain('"SELECT occurrence_key,start_local,end_local FROM calendar_occurrence_exceptions WHERE');
    expect(availability).toContain("occurrenceExceptionsSql('occurrence_key,start_local,end_local', 'cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL')");

    const reminders = await readFile(src('calendar/reminder-purchases.ts'), 'utf8');
    expect(reminders).toContain("occurrenceExceptionPointSql('cancelled,start_local')");
  });

  it('emits the original byte-identical SQL for every parameterized caller', () => {
    expect(occurrenceExceptionPointSql('start_local,end_local,cancelled')).toBe(original.availabilityPoint);
    expect(occurrenceExceptionPointSql('cancelled,start_local')).toBe(original.reminderPoint);
    expect(occurrenceExceptionsSql('occurrence_key,start_local,end_local', 'cancelled=false AND start_local IS NOT NULL AND end_local IS NOT NULL')).toBe(original.movedOccurrences);
  });

  it('leaves the r1-r27 helpers, locks and work-item sources untouched', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'overlapProbe', 'occurrenceExceptionPointSql',
      'reminderAttendeesSql', 'reminderAttendeePointSql', 'attendeeScopeExistsSql',
      'participantScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql', 'calendarImageSql',
      'appointmentAttendeeSeriesSql', 'appointmentChildImageSql', 'commandReceiptSql',
      'derivedProbeSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain("derivedProbeSql(scopeRowSql('company_id,?::text branch_id,?::text department_id'),'workforce_actor_scope',where)");
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain('SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE');
    const layouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(layouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect((await readFile(src('work-items.ts'), 'utf8')).length).toBeGreaterThan(0);
  });

  it('retirement:ts-statements stays strictly below the r27 BEFORE=54 baseline', () => {
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
    expect(count).toBeLessThanOrEqual(53);
    expect(count).toBeLessThan(54);
  });
});
