import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const attendeeSql =
  'SELECT 1 FROM calendar_attendees WHERE appointment_id=? AND company_id=? AND user_id=?';
const appointmentPrefix = 'SELECT 1 FROM calendar_appointments WHERE id=? AND company_id=?';

describe('work_projection read-tail r12 dedupe', () => {
  it('keeps one canonical definition per shared probe and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const invitedAttendeeSql =')).toHaveLength(2);
    expect(shared.split('export const appointmentProbe =')).toHaveLength(2);
    for (const name of ['calendar/purchases.ts', 'collaboration-inbox-sources.ts']) {
      const source = await readFile(src(name), 'utf8');
      expect(source).not.toContain(attendeeSql);
      expect(source).not.toContain(appointmentPrefix);
      expect(source).toMatch(/appointmentProbe|invitedAttendeeSql/);
    }
  });

  it('emits the original byte-identical probes for every caller', async () => {
    const { appointmentProbe, invitedAttendeeSql } = await import(
      '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts'
    );
    expect(invitedAttendeeSql).toBe(attendeeSql);
    expect(appointmentProbe('1')).toBe(appointmentPrefix);
    expect(appointmentProbe('id,version')).toBe(
      'SELECT id,version FROM calendar_appointments WHERE id=? AND company_id=?',
    );
    expect(appointmentProbe('1', ' AND created_by=?')).toBe(
      `${appointmentPrefix} AND created_by=?`,
    );
  });

  it('leaves the calendar and daily-plan lock statements untouched', async () => {
    expect(await readFile(src('calendar/purchases.ts'), 'utf8')).toContain(
      'calendarManualSourceReadLocks',
    );
    expect(await readFile(src('daily-plan.ts'), 'utf8')).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(await readFile(src('page-layouts.ts'), 'utf8')).toContain(
      'SELECT pg_advisory_xact_lock(hashtextextended(?,0))',
    );
  });
});
