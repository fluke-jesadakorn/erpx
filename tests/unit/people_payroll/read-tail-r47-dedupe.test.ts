import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {ppRd42FourColOwnerProjection} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r42-sources';
import {ppRd46NamedJoinSource} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r46-sources';
import {ppRd47GraphSnapshotProjection} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r47-sources';
import {platformOwnerProjectionSources} from '../../../packages/server-people_payroll/src/adapters/platform-owner-projection-sources';
import {collaborationNotificationSnapshotSources} from '../../../packages/server-people_payroll/src/adapters/collaboration-notification-snapshot-sources';
import {organizationGraphSnapshotSources} from '../../../packages/server-people_payroll/src/adapters/organization-graph-snapshot-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

const render = {
  ppRd42FourColOwnerProjection: (a: string[]) => ppRd42FourColOwnerProjection(a[0], a[1]),
  ppRd46NamedJoinSource: (a: string[]) => ppRd46NamedJoinSource(a[0], a[1], a[2]),
  ppRd47GraphSnapshotProjection: (a: string[]) => ppRd47GraphSnapshotProjection(a[0], a[1]),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; call: string };

// Each literal is copied verbatim from the pre-r47 inline call site. Byte-identity
// oracles only; they are never live query sources. No bind was introduced or moved.
const oracles: Row[] = [
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","user_id","company_id","branch_id","display_name","status","created_at","department_id","person_id"', 'employees'],
    literal: 'SELECT "id","user_id","company_id","branch_id","display_name","status","created_at","department_id","person_id" FROM employees',
    call: `ppRd42FourColOwnerProjection('"id","user_id","company_id","branch_id","display_name","status","created_at","department_id","person_id"','employees')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","company_id","name","country_code","currency","frequency","timezone","calendar_json","status","created_by","created_at","updated_at","overtime_multiplier_bps","proration_method","schedule_enabled","pay_delay_days","schedule_anchor_date","schedule_version"', 'hr_pay_groups'],
    literal: 'SELECT "id","company_id","name","country_code","currency","frequency","timezone","calendar_json","status","created_by","created_at","updated_at","overtime_multiplier_bps","proration_method","schedule_enabled","pay_delay_days","schedule_anchor_date","schedule_version" FROM hr_pay_groups',
    call: `ppRd42FourColOwnerProjection('"id","company_id","name","country_code","currency","frequency","timezone","calendar_json","status","created_by","created_at","updated_at","overtime_multiplier_bps","proration_method","schedule_enabled","pay_delay_days","schedule_anchor_date","schedule_version"','hr_pay_groups')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","request_id","company_id","leave_date","segment","units","source"', 'leave_day_segments'],
    literal: 'SELECT "id","request_id","company_id","leave_date","segment","units","source" FROM leave_day_segments',
    call: `ppRd42FourColOwnerProjection('"id","request_id","company_id","leave_date","segment","units","source"','leave_day_segments')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","company_id","engagement_id","policy_version_id","holiday_calendar_id","effective_from","effective_to","created_by","created_at","version"', 'leave_policy_assignments'],
    literal: 'SELECT "id","company_id","engagement_id","policy_version_id","holiday_calendar_id","effective_from","effective_to","created_by","created_at","version" FROM leave_policy_assignments',
    call: `ppRd42FourColOwnerProjection('"id","company_id","engagement_id","policy_version_id","holiday_calendar_id","effective_from","effective_to","created_by","created_at","version"','leave_policy_assignments')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"policy_version_id","leave_type_id","grant_kind","grant_units","accrual_units","accrual_period","carryover_limit","employee_eligible","contractor_eligible","evidence_min_units","evidence_purpose","evidence_min_count","evidence_due_stage","evidence_grace_days","evidence_note","evidence_privacy"', 'leave_policy_rules'],
    literal: 'SELECT "policy_version_id","leave_type_id","grant_kind","grant_units","accrual_units","accrual_period","carryover_limit","employee_eligible","contractor_eligible","evidence_min_units","evidence_purpose","evidence_min_count","evidence_due_stage","evidence_grace_days","evidence_note","evidence_privacy" FROM leave_policy_rules',
    call: `ppRd42FourColOwnerProjection('"policy_version_id","leave_type_id","grant_kind","grant_units","accrual_units","accrual_period","carryover_limit","employee_eligible","contractor_eligible","evidence_min_units","evidence_purpose","evidence_min_count","evidence_due_stage","evidence_grace_days","evidence_note","evidence_privacy"','leave_policy_rules')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","company_id","policy_key","version","name","status","effective_from","effective_to","timezone","workweek_json","allow_negative","created_by","created_at"', 'leave_policy_versions'],
    literal: 'SELECT "id","company_id","policy_key","version","name","status","effective_from","effective_to","timezone","workweek_json","allow_negative","created_by","created_at" FROM leave_policy_versions',
    call: `ppRd42FourColOwnerProjection('"id","company_id","policy_key","version","name","status","effective_from","effective_to","timezone","workweek_json","allow_negative","created_by","created_at"','leave_policy_versions')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","company_id","code","name","description","tracked","evidence_after_units","evidence_note","private_evidence","active","effective_from","effective_to","created_by","created_at","updated_at"', 'leave_types'],
    literal: 'SELECT "id","company_id","code","name","description","tracked","evidence_after_units","evidence_note","private_evidence","active","effective_from","effective_to","created_by","created_at","updated_at" FROM leave_types',
    call: `ppRd42FourColOwnerProjection('"id","company_id","code","name","description","tracked","evidence_after_units","evidence_note","private_evidence","active","effective_from","effective_to","created_by","created_at","updated_at"','leave_types')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","company_id","person_id","engagement_id","position_id","package_id","package_revision","definition","source_kind","scope","case_id","operation_id","authorizing_assignment_id","valid_from","valid_until","ended_at","created_at"', 'people_applied_packages'],
    literal: 'SELECT "id","company_id","person_id","engagement_id","position_id","package_id","package_revision","definition","source_kind","scope","case_id","operation_id","authorizing_assignment_id","valid_from","valid_until","ended_at","created_at" FROM people_applied_packages',
    call: `ppRd42FourColOwnerProjection('"id","company_id","person_id","engagement_id","position_id","package_id","package_revision","definition","source_kind","scope","case_id","operation_id","authorizing_assignment_id","valid_from","valid_until","ended_at","created_at"','people_applied_packages')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"company_id","package_id","revision","definition","created_by","created_at"', 'people_package_revisions'],
    literal: 'SELECT "company_id","package_id","revision","definition","created_by","created_at" FROM people_package_revisions',
    call: `ppRd42FourColOwnerProjection('"company_id","package_id","revision","definition","created_by","created_at"','people_package_revisions')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"company_id","position_id","package_id","revision"', 'people_position_packages'],
    literal: 'SELECT "company_id","position_id","package_id","revision" FROM people_position_packages',
    call: `ppRd42FourColOwnerProjection('"company_id","position_id","package_id","revision"','people_position_packages')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","shift_id","person_id","engagement_id","state","version"', 'people_roster_assignments'],
    literal: 'SELECT "id","shift_id","person_id","engagement_id","state","version" FROM people_roster_assignments',
    call: `ppRd42FourColOwnerProjection('"id","shift_id","person_id","engagement_id","state","version"','people_roster_assignments')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","week_id","template_id","name","local_date","start_time","end_time","starts_at","ends_at","capacity","state","version"', 'people_roster_shifts'],
    literal: 'SELECT "id","week_id","template_id","name","local_date","start_time","end_time","starts_at","ends_at","capacity","state","version" FROM people_roster_shifts',
    call: `ppRd42FourColOwnerProjection('"id","week_id","template_id","name","local_date","start_time","end_time","starts_at","ends_at","capacity","state","version"','people_roster_shifts')`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd42FourColOwnerProjection',
    args: ['"id","company_id","branch_id","department_id","week_start","timezone","state","version","created_by","published_by","created_at","updated_at"', 'people_roster_weeks'],
    literal: 'SELECT "id","company_id","branch_id","department_id","week_start","timezone","state","version","created_by","published_by","created_at","updated_at" FROM people_roster_weeks',
    call: `ppRd42FourColOwnerProjection('"id","company_id","branch_id","department_id","week_start","timezone","state","version","created_by","published_by","created_at","updated_at"','people_roster_weeks')`,
  },
  {
    file: 'collaboration-notification-snapshot-sources.ts',
    sym: 'ppRd46NamedJoinSource',
    args: ['"id", "week_id", "template_id", "name", "local_date", "start_time", "end_time", "starts_at", "ends_at", "capacity", "state", "version"', 'people_roster_shifts', 's'],
    literal: 'JOIN (SELECT "id", "week_id", "template_id", "name", "local_date", "start_time", "end_time", "starts_at", "ends_at", "capacity", "state", "version" FROM people_roster_shifts) s',
    call: `ppRd46NamedJoinSource('"id", "week_id", "template_id", "name", "local_date", "start_time", "end_time", "starts_at", "ends_at", "capacity", "state", "version"','people_roster_shifts','s')`,
  },
  {
    file: 'collaboration-notification-snapshot-sources.ts',
    sym: 'ppRd46NamedJoinSource',
    args: ['"id", "company_id", "branch_id", "department_id", "week_start", "timezone", "state", "version", "created_by", "published_by", "created_at", "updated_at"', 'people_roster_weeks', 'w'],
    literal: 'JOIN (SELECT "id", "company_id", "branch_id", "department_id", "week_start", "timezone", "state", "version", "created_by", "published_by", "created_at", "updated_at" FROM people_roster_weeks) w',
    call: `ppRd46NamedJoinSource('"id", "company_id", "branch_id", "department_id", "week_start", "timezone", "state", "version", "created_by", "published_by", "created_at", "updated_at"','people_roster_weeks','w')`,
  },
  {
    file: 'organization-graph-snapshot-sources.ts',
    sym: 'ppRd47GraphSnapshotProjection',
    args: ['person', 'hr_people'],
    literal: 'SELECT person.id,person.company_id,to_jsonb(person) snapshot FROM hr_people person',
    call: `ppRd47GraphSnapshotProjection('person','hr_people')`,
  },
  {
    file: 'organization-graph-snapshot-sources.ts',
    sym: 'ppRd47GraphSnapshotProjection',
    args: ['engagement', 'hr_engagements'],
    literal: 'SELECT engagement.id,engagement.company_id,to_jsonb(engagement) snapshot FROM hr_engagements engagement',
    call: `ppRd47GraphSnapshotProjection('engagement','hr_engagements')`,
  },
];

describe('People Payroll read-tail r47 three-family dedupe', () => {
  test('the three shared builders own byte-identical, read-only literals', () => {
    expect(ppRd42FourColOwnerProjection('a,b,c,d', 't')).toBe('SELECT a,b,c,d FROM t');
    expect(ppRd42FourColOwnerProjection('a,b,c,d', 't').match(/\?/g)).toBeNull();
    expect(ppRd46NamedJoinSource('"a"', 't', 'x')).toBe('JOIN (SELECT "a" FROM t) x');
    expect(ppRd46NamedJoinSource('"a"', 't', 'x').match(/\?/g)).toBeNull();
    expect(ppRd47GraphSnapshotProjection('p', 'hr_people')).toBe('SELECT p.id,p.company_id,to_jsonb(p) snapshot FROM hr_people p');
    expect(ppRd47GraphSnapshotProjection('p', 'hr_people').match(/\?/g)).toBeNull();
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(17);
  });

  test('the three exported sources still emit the captured bytes at runtime', () => {
    const p = platformOwnerProjectionSources as Record<string, string>;
    const c = collaborationNotificationSnapshotSources() as Record<string, string>;
    const g = organizationGraphSnapshotSources as Record<string, string>;
    const actual: Record<string, Record<string, string>> = {
      'platform-owner-projection-sources.ts': p,
      'collaboration-notification-snapshot-sources.ts': c,
      'organization-graph-snapshot-sources.ts': g,
    };
    for (const o of oracles) expect(Object.values(actual[o.file]), o.file + ' byte oracle').toContain(o.literal);
  });

  test('each caller uses its builder, keeps no inline head and keeps its binds', () => {
    expect((src('pp-read-tail-r47-sources.ts').match(/export const ppRd47/g) ?? []).length).toBe(1);
    const expectedCounts: Record<string, [Sym, number][]> = {
      'platform-owner-projection-sources.ts': [['ppRd42FourColOwnerProjection', 13]],
      'collaboration-notification-snapshot-sources.ts': [['ppRd46NamedJoinSource', 2]],
      'organization-graph-snapshot-sources.ts': [['ppRd47GraphSnapshotProjection', 2]],
    };
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.literal);
    }
    expect(src('platform-owner-projection-sources.ts')).toContain("from './pp-read-tail-r42-sources'");
    expect(src('collaboration-notification-snapshot-sources.ts')).toContain("from './pp-read-tail-r46-sources'");
    expect(src('organization-graph-snapshot-sources.ts')).toContain("from './pp-read-tail-r47-sources'");
    for (const [name, pairs] of Object.entries(expectedCounts)) {
      for (const [sym, count] of pairs) expect(uses(name, sym), name + ' ' + sym).toBe(count);
    }
  });

  test('locks, advisory, native kinds, cross-owner projections and r1-r46 builders stay untouched', () => {
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT id FROM hr_time_entries WHERE id=? FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('platform-owner-projection-sources.ts')).toContain('ppRd45CompRevisionProjection');
    expect(src('platform-owner-projection-sources.ts')).toContain('PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL');
    expect(src('collaboration-notification-snapshot-sources.ts')).toContain('ppRd44NamedSourceProjection');
    expect(src('collaboration-notification-snapshot-sources.ts')).toContain('PP_RD20_HR_ENGAGEMENTS_FROM_SQL');
    expect(src('pp-read-tail-r42-sources.ts')).toContain('export const ppRd42FourColOwnerProjection');
    expect(src('pp-read-tail-r46-sources.ts')).toContain('export const ppRd46NamedJoinSource');
    expect(src('pp-read-tail-r45-sources.ts')).toContain('export const ppRd45CompRevisionProjection');
    for (const name of ['platform-owner-projection-sources.ts', 'collaboration-notification-snapshot-sources.ts', 'organization-graph-snapshot-sources.ts']) {
      expect(src(name), name + ' has no foreign r47 symbol').not.toMatch(/ppRd47(?!GraphSnapshotProjection)/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1261', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1261);
    expect(total).toBe(1175);
  });
});
