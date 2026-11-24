import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  purchase:
    "SELECT 1 FROM command_receipts WHERE command_id=? AND actor_id=? AND operation=? AND record_id=? AND authority_json::jsonb->>'companyId'=?",
  reminder:
    "SELECT 1 FROM command_receipts r WHERE r.command_id=? AND r.actor_id=? AND r.operation=? AND r.authority_json::jsonb->>'companyId'=? AND (r.result_json::jsonb->>'id'=? OR r.record_id=? OR ?='calendar-'||r.command_id AND r.operation='calendar.appointment.reschedule') AND (r.result_json::jsonb->>'version')::bigint>=?",
} as const;

describe('work_projection read-tail r22 dedupe', () => {
  it('keeps one canonical command receipt builder and no inline command_receipts copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const commandReceiptSql =')).toHaveLength(2);

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    const reminders = await readFile(src('calendar/reminder-authority.ts'), 'utf8');
    expect(purchases).toContain('commandReceiptSql(');
    expect(reminders).toContain('commandReceiptSql(');
    expect(purchases).not.toContain('FROM command_receipts WHERE');
    expect(reminders).not.toContain('FROM command_receipts r WHERE');
  });

  it('emits the original SQL for both parameterized callers', async () => {
    const { commandReceiptSql } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    expect(commandReceiptSql(' AND record_id=?')).toBe(original.purchase);
    expect(
      commandReceiptSql(
        '',
        " AND (r.result_json::jsonb->>'id'=? OR r.record_id=? OR ?='calendar-'||r.command_id AND r.operation='calendar.appointment.reschedule') AND (r.result_json::jsonb->>'version')::bigint>=?",
        'r',
      ),
    ).toBe(original.reminder);
  });

  it('leaves the r9-r21 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'overlapProbe', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql',
      'calendarImageSql', 'appointmentAttendeeSeriesSql', 'appointmentChildImageSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(purchases).toContain('nativeClearCommandAssertions');
    expect(purchases).toContain('appointmentChildImageSql(');
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
    expect(reminders).toContain('readOccurrenceResponse(db,facts)');
    expect(reminders).toContain('nativeOwnerWriteRow');
  });

  it('retirement:ts-statements stays strictly below the r22 BEFORE=60 baseline', () => {
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
    expect(count).toBeLessThan(60);
  });
});
