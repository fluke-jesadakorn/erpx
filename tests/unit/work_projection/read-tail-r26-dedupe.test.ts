import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import {
  inList,
  attendeeScopeExistsSql,
  participantScopeExistsSql,
} from '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const engagementSource = 'SELECT person_id,company_id,status,branch_id,department_id FROM hr_engagements';
const engagementBody =
  `JOIN (${engagementSource}) e ON e.person_id=ap.person_id AND e.company_id=ap.company_id ` +
  `WHERE ap.appointment_id=x.id AND e.status='active' AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}`;
const personBody = 'WHERE ap.appointment_id=x.id AND ap.person_id=?';
const original = {
  engagementArm: `EXISTS(SELECT 1 FROM calendar_appointment_participants ap ${engagementBody})`,
  personArm: `EXISTS(SELECT 1 FROM calendar_appointment_participants ap ${personBody})`,
} as const;

describe('work_projection read-tail r26 dedupe', () => {
  it('keeps one canonical participant-scope arm and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const participantScopeExistsSql =')).toHaveLength(2);

    const facts = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(facts).not.toContain('EXISTS(SELECT 1 FROM calendar_appointment_participants ap JOIN');
    expect(facts).not.toContain('EXISTS(SELECT 1 FROM calendar_appointment_participants ap WHERE');
    expect(facts).toContain('${participantScopeExistsSql(`JOIN (');
    expect(facts).toContain("${participantScopeExistsSql('WHERE ap.appointment_id=x.id AND ap.person_id=?')}");
    expect(facts.split('${attendeeScopeExistsSql}')).toHaveLength(4);
  });

  it('emits the original byte-identical arm for both parameterized callers', () => {
    expect(participantScopeExistsSql(engagementBody)).toBe(original.engagementArm);
    expect(participantScopeExistsSql(personBody)).toBe(original.personArm);
    expect(attendeeScopeExistsSql).toBe(
      'EXISTS(SELECT 1 FROM calendar_attendees ap WHERE ap.appointment_id=x.id AND ap.user_id=?)',
    );
  });

  it('leaves the r1-r25 helpers, locks and work-item sources untouched', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'overlapProbe', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'reminderAttendeesSql', 'attendeeScopeExistsSql',
      'receiptByIdSql', 'dailyPlanSql', 'calendarImageSql',
      'appointmentAttendeeSeriesSql', 'appointmentChildImageSql', 'commandReceiptSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    expect(shared.match(/`SELECT \$\{columns\} FROM calendar_appointments a JOIN calendar_attendees t ON t\.appointment_id=a\.id AND t\.company_id=a\.company_id WHERE a\.id=\? AND a\.company_id=\?\$\{tail\}`/g)).toHaveLength(1);

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).toContain(
      'LOCK TABLE calendar_appointments,calendar_announcements,calendar_appointment_participants,calendar_attendees,calendar_series,calendar_occurrence_exceptions,calendar_occurrence_responses,calendar_reminder_deliveries,calendar_history,calendar_reminder_execution_authorizations IN SHARE ROW EXCLUSIVE MODE',
    );
    const daily = await readFile(src('daily-plan.ts'), 'utf8');
    expect(daily).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    const layouts = await readFile(src('page-layouts.ts'), 'utf8');
    expect(layouts).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect((await readFile(src('work-items.ts'), 'utf8')).length).toBeGreaterThan(0);
  });

  it('retirement:ts-statements stays strictly below the r25 BEFORE=56 baseline', () => {
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
    expect(count).toBeLessThanOrEqual(54);
    expect(count).toBeLessThan(55);
  });
});
