import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  appointment:
    'SELECT company_id,branch_id,department_id,created_by,organizer_person_id FROM calendar_appointments WHERE id=? AND company_id=?',
  announcement:
    'SELECT company_id,branch_id,department_id,created_by FROM calendar_announcements WHERE id=? AND company_id=?',
} as const;

describe('work_projection read-tail r24 dedupe', () => {
  it('keeps one canonical source-row point-read builder and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const appointmentProbe =')).toHaveLength(2);
    expect(shared.match(/`SELECT \$\{columns\} FROM \$\{table\} WHERE id=\? AND company_id=\?\$\{extra\}`/g)).toHaveLength(1);

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).not.toContain('SELECT company_id,branch_id,department_id,created_by');
    expect(purchases).toContain(
      "appointmentProbe(`company_id,branch_id,department_id,created_by${type==='appointment'?',organizer_person_id':''}`,'',type==='appointment'?'calendar_appointments':'calendar_announcements')",
    );
  });

  it('emits the original byte-identical SQL for every source-row caller', async () => {
    const { appointmentProbe } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    expect(
      appointmentProbe(
        'company_id,branch_id,department_id,created_by,organizer_person_id',
        '',
        'calendar_appointments',
      ),
    ).toBe(original.appointment);
    expect(
      appointmentProbe(
        'company_id,branch_id,department_id,created_by',
        '',
        'calendar_announcements',
      ),
    ).toBe(original.announcement);
    // r12 callers keep their exact original SQL under the new optional table arm.
    expect(appointmentProbe('1')).toBe('SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=?');
    expect(appointmentProbe('id,version')).toBe('SELECT id,version FROM calendar_appointments WHERE id=? AND company_id=?');
    expect(appointmentProbe('1', ' AND created_by=?')).toBe(
      'SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=? AND created_by=?',
    );
  });

  it('leaves the r9-r23 helpers, locks and retained consumers intact', async () => {
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

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(purchases).toContain('calendarManualSourceReadLocks');
    expect(purchases).toContain('commandReceiptSql(');
    expect(purchases).toContain('appointmentProbe(\'1\')');
    expect(purchases).toContain('versionById(');
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain('SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE');
    const layouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(layouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
  });

  it('retirement:ts-statements stays strictly below the r24 BEFORE=58 baseline', () => {
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
    expect(count).toBeLessThan(58);
  });
});
