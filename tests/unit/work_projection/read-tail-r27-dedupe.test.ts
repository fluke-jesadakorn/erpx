import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import {
  inList,
  derivedProbeSql,
} from '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const people = 'PEOPLE_SOURCE';
const engagements = 'ENGAGEMENTS_SOURCE';
const original = {
  identityProbe: `SELECT 1 FROM (${people}) person WHERE person.id=? AND person.company_id=? AND iam_same_person(person.user_id,?,person.company_id)`,
  engagementProbe: `SELECT 1 FROM (${engagements}) e WHERE e.person_id=p.id AND e.company_id=p.company_id AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}`,
} as const;

describe('work_projection read-tail r27 dedupe', () => {
  it('keeps one canonical derived-probe SELECT head and no inline derived-source copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const derivedProbeSql =')).toHaveLength(2);
    expect(shared.match(/`SELECT 1 FROM \(\$\{source\}\) \$\{alias\} WHERE \$\{predicate\}`/g)).toHaveLength(1);

    const purchases = await readFile(src('calendar/purchases.ts'), 'utf8');
    expect(purchases).not.toContain('`SELECT 1 FROM (${calendarPeopleProjectionSources.people}) person WHERE');
    expect(purchases).toContain("derivedProbeSql(calendarPeopleProjectionSources.people, 'person',");

    const facts = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(facts).not.toContain('EXISTS(SELECT 1 FROM (${calendarPeopleProjectionSources.engagements}) e WHERE');
    expect(facts).toContain("EXISTS(${derivedProbeSql(calendarPeopleProjectionSources.engagements, 'e',");
  });

  it('emits the original byte-identical SQL for every parameterized caller', () => {
    expect(derivedProbeSql(people, 'person', 'person.id=? AND person.company_id=? AND iam_same_person(person.user_id,?,person.company_id)')).toBe(original.identityProbe);
    expect(derivedProbeSql(engagements, 'e', `e.person_id=p.id AND e.company_id=p.company_id AND ${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}`)).toBe(original.engagementProbe);
  });

  it('leaves the r1-r26 helpers, locks, r10-pinned actor scope and work-item sources untouched', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'reminderAttendeesSql', 'attendeeScopeExistsSql',
      'participantScopeExistsSql', 'receiptByIdSql', 'dailyPlanSql', 'calendarImageSql',
      'appointmentAttendeeSeriesSql', 'appointmentChildImageSql', 'commandReceiptSql',
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

  it('retirement:ts-statements stays strictly below the r26 BEFORE=55 baseline', () => {
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
