import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd45CompRevisionProjection,
  ppRd45EngagementProjection,
  ppRd45PeopleProjection,
  ppRd45ReceiptMarkerProjection,
  ppRd45RunByIdProjection,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r45-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

const render = {
  ppRd45EngagementProjection: (a: string[]) => ppRd45EngagementProjection(a[0]),
  ppRd45RunByIdProjection: (a: string[]) => ppRd45RunByIdProjection(a[0]),
  ppRd45PeopleProjection: (a: string[]) => ppRd45PeopleProjection(a[0]),
  ppRd45CompRevisionProjection: (a: string[]) => ppRd45CompRevisionProjection(a[0]),
  ppRd45ReceiptMarkerProjection: (a: string[]) => ppRd45ReceiptMarkerProjection(a[0], a[1]),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; call: string };

// Each literal is copied verbatim from the pre-r45 inline head. Byte-identity
// oracles only; they are never live query sources. Bind arrays stay unchanged.
const compCols = '"id","engagement_id","effective_from","effective_to","pay_frequency","base_amount_minor","hourly_rate_minor","expected_hours","currency","components_json","status","version","approved_by","created_by","created_at","updated_at","daily_rate_minor","standard_daily_hours"';
const runCols = 'id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,version,revision,input_digest,submitted_by,submitted_digest,adjustment_of_run_id';
const oracles: Row[] = [
  // Family 1 — engagement projection (4 -> 1, -3)
  {
    file: 'benefits-owner-facts.ts',
    sym: 'ppRd45EngagementProjection',
    args: ['id,person_id,company_id,branch_id,department_id,status,version,start_date'],
    literal: 'SELECT id,person_id,company_id,branch_id,department_id,status,version,start_date FROM hr_engagements',
    call: `ppRd45EngagementProjection("id,person_id,company_id,branch_id,department_id,status,version,start_date")`,
  },
  {
    file: 'calendar-people-projection-sources.ts',
    sym: 'ppRd45EngagementProjection',
    args: ['person_id,company_id,status,branch_id,department_id'],
    literal: 'SELECT person_id,company_id,status,branch_id,department_id FROM hr_engagements',
    call: `ppRd45EngagementProjection("person_id,company_id,status,branch_id,department_id")`,
  },
  {
    file: 'package-obligations.ts',
    sym: 'ppRd45EngagementProjection',
    args: ['*'],
    literal: 'SELECT * FROM hr_engagements',
    call: `ppRd45EngagementProjection("*")`,
  },
  {
    file: 'people-purchases.ts',
    sym: 'ppRd45EngagementProjection',
    args: ['id,company_id,person_id,id engagement_id'],
    literal: 'SELECT id,company_id,person_id,id engagement_id FROM hr_engagements',
    call: `ppRd45EngagementProjection("id,company_id,person_id,id engagement_id")`,
  },
  // Family 2 — run by id (2 -> 1, -1)
  {
    file: 'payroll-readiness.ts',
    sym: 'ppRd45RunByIdProjection',
    args: [runCols],
    literal: `SELECT ${runCols} FROM hr_payroll_runs WHERE id=? AND company_id=?`,
    call: `ppRd45RunByIdProjection("${runCols}")`,
  },
  {
    file: 'people-payroll.ts',
    sym: 'ppRd45RunByIdProjection',
    args: ['id,status,currency'],
    literal: 'SELECT id,status,currency FROM hr_payroll_runs WHERE id=? AND company_id=?',
    call: `ppRd45RunByIdProjection("id,status,currency")`,
  },
  // Family 3 — people projection (2 -> 1, -1)
  {
    file: 'calendar-people-projection-sources.ts',
    sym: 'ppRd45PeopleProjection',
    args: ['id,user_id,company_id,legal_name,merged_into_id'],
    literal: 'SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people',
    call: `ppRd45PeopleProjection("id,user_id,company_id,legal_name,merged_into_id")`,
  },
  {
    file: 'people-purchases.ts',
    sym: 'ppRd45PeopleProjection',
    args: ['id,company_id,id person_id,NULL::text engagement_id'],
    literal: 'SELECT id,company_id,id person_id,NULL::text engagement_id FROM hr_people',
    call: `ppRd45PeopleProjection("id,company_id,id person_id,NULL::text engagement_id")`,
  },
  // Family 4 — comp revision projection (2 -> 1, -1)
  {
    file: 'package-obligations.ts',
    sym: 'ppRd45CompRevisionProjection',
    args: ['*'],
    literal: 'SELECT * FROM hr_compensation_revisions',
    call: `ppRd45CompRevisionProjection("*")`,
  },
  {
    file: 'platform-owner-projection-sources.ts',
    sym: 'ppRd45CompRevisionProjection',
    args: [compCols],
    literal: `SELECT ${compCols} FROM hr_compensation_revisions`,
    call: `ppRd45CompRevisionProjection("${compCols.replace(/"/g, '\\"')}")`,
  },
  // Family 5 — receipt marker projection (2 -> 1, -1)
  {
    file: 'project-worker-startup-source-sql.ts',
    sym: 'ppRd45ReceiptMarkerProjection',
    args: ['record_id person_id', '_projectWorkerStartup'],
    literal: `SELECT record_id person_id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE jsonb_exists(result_json::jsonb,'_projectWorkerStartup')`,
    call: `ppRd45ReceiptMarkerProjection('record_id person_id',projectWorkerMarker)`,
  },
  {
    file: 'project-worker-startup-source-sql.ts',
    sym: 'ppRd45ReceiptMarkerProjection',
    args: ['record_id id', '_projectWorkerPlacement'],
    literal: `SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE jsonb_exists(result_json::jsonb,'_projectWorkerPlacement')`,
    call: `ppRd45ReceiptMarkerProjection('record_id id',projectWorkerPlacementMarker)`,
  },
];

describe('People Payroll read-tail r45 five-family dedupe', () => {
  test('the five shared builders own byte-identical, read-only literals', () => {
    expect(ppRd45EngagementProjection('"a"')).toBe('SELECT "a" FROM hr_engagements');
    expect(ppRd45RunByIdProjection('a')).toBe('SELECT a FROM hr_payroll_runs WHERE id=? AND company_id=?');
    expect(ppRd45PeopleProjection('a')).toBe('SELECT a FROM hr_people');
    expect(ppRd45CompRevisionProjection('a')).toBe('SELECT a FROM hr_compensation_revisions');
    expect(ppRd45ReceiptMarkerProjection('record_id id', 'marker')).toBe(
      `SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE jsonb_exists(result_json::jsonb,'marker')`,
    );
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.sym).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(12);
  });

  test('each caller uses its builder, keeps no inline head and keeps its binds', () => {
    const shared = src('pp-read-tail-r45-sources.ts');
    expect((shared.match(/export const ppRd45/g) ?? []).length).toBe(5);
    const expectedCounts: Record<string, [Sym, number][]> = {
      'benefits-owner-facts.ts': [['ppRd45EngagementProjection', 1]],
      'calendar-people-projection-sources.ts': [['ppRd45PeopleProjection', 1], ['ppRd45EngagementProjection', 1]],
      'package-obligations.ts': [['ppRd45EngagementProjection', 1], ['ppRd45CompRevisionProjection', 1]],
      'people-purchases.ts': [['ppRd45PeopleProjection', 1], ['ppRd45EngagementProjection', 1]],
      'payroll-readiness.ts': [['ppRd45RunByIdProjection', 1]],
      'people-payroll.ts': [['ppRd45RunByIdProjection', 1]],
      'platform-owner-projection-sources.ts': [['ppRd45CompRevisionProjection', 1]],
      'project-worker-startup-source-sql.ts': [['ppRd45ReceiptMarkerProjection', 2]],
    };
    for (const o of oracles) expect(src(o.file), o.file + ' builder call').toContain(o.call);
    for (const name of ['benefits-owner-facts.ts', 'calendar-people-projection-sources.ts', 'package-obligations.ts', 'people-purchases.ts', 'payroll-readiness.ts', 'people-payroll.ts', 'platform-owner-projection-sources.ts', 'project-worker-startup-source-sql.ts']) {
      expect(src(name), name + ' imports r45 builders').toContain("from './pp-read-tail-r45-sources'");
    }
    expect(src('benefits-owner-facts.ts')).not.toContain('engagements:"SELECT');
    expect(src('calendar-people-projection-sources.ts')).not.toContain('people:"SELECT');
    expect(src('calendar-people-projection-sources.ts')).not.toContain('engagements:"SELECT');
    expect(src('package-obligations.ts')).not.toContain('engagements:"SELECT');
    expect(src('package-obligations.ts')).not.toContain('compensation:"SELECT');
    expect(src('people-purchases.ts')).not.toContain('?"SELECT id,company_id,id person_id,NULL::text engagement_id FROM hr_people"');
    expect(src('people-purchases.ts')).not.toContain('?"SELECT id,company_id,person_id,id engagement_id FROM hr_engagements"');
    expect(src('payroll-readiness.ts')).not.toContain('"SELECT id,company_id,pay_group_id,country_pack_id');
    expect(src('people-payroll.ts')).not.toContain('"SELECT id,status,currency FROM hr_payroll_runs WHERE id=? AND company_id=?"');
    expect(src('platform-owner-projection-sources.ts')).not.toContain('hr_compensation_revisions: "SELECT');
    expect(src('project-worker-startup-source-sql.ts')).not.toContain('projectWorkerMarkedRows=`SELECT');
    expect(src('project-worker-startup-source-sql.ts')).not.toContain('projectWorkerPlacementMarkedRows=`SELECT');
    expect(src('payroll-readiness.ts')).toContain('.bind(runId, companyId).first<Run>()');
    expect(src('people-payroll.ts')).toContain('.bind(sourceId, companyId).first<{');
    expect(src('people-purchases.ts')).toContain('.bind(target.id).first<Source>()');
    expect(src('project-worker-startup-source-sql.ts')).toContain("ppRd45ReceiptMarkerProjection('record_id person_id',projectWorkerMarker)");
    expect(src('project-worker-startup-source-sql.ts')).toContain("ppRd45ReceiptMarkerProjection('record_id id',projectWorkerPlacementMarker)");
    for (const [name, pairs] of Object.entries(expectedCounts)) {
      for (const [sym, count] of pairs) expect(uses(name, sym), name + ' ' + sym).toBe(count);
    }
  });

  test('locks, native kinds, cross-owner projections and r1-r44 builders stay untouched', () => {
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('payroll-other-remittance.ts')).toContain('SELECT id FROM hr_payroll_runs WHERE id=? AND company_id=? FOR UPDATE');
    expect(src('payroll-remittance.ts')).toContain('SELECT id FROM hr_payroll_runs WHERE id=? AND company_id=? FOR UPDATE');
    expect(src('payroll-purchases.ts')).toContain('FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('pp-read-tail-r49-sources.ts')).toContain('filesProjectionSources.attachments');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('filesProjectionSources.attachments');
    expect(src('pp-read-tail-r44-sources.ts')).toContain('export const ppRd44NamedSourceProjection');
    expect(src('pp-read-tail-r43-sources.ts')).toContain('export const ppRd43ActiveLeavePolicyProjection');
    expect(src('pp-read-tail-r42-sources.ts')).toContain('export const ppRd42FourColOwnerProjection');
    expect(src('pp-read-tail-r40-sources.ts')).toContain('export const ppRd40UnlinkedUserByCompany');
    expect(src('pp-read-tail-r38-sources.ts')).toContain('export const ppRd38SinglePredTail');
    expect(src('pp-read-tail-r32-sources.ts')).toContain('export const ppRd32TwoColProjection');
    expect(src('pp-read-tail-r20-sources.ts')).toContain('PP_RD20_HR_ENGAGEMENTS_FROM_SQL');
    for (const name of ['benefits-owner-facts.ts', 'calendar-people-projection-sources.ts', 'package-obligations.ts', 'people-purchases.ts', 'payroll-readiness.ts', 'people-payroll.ts', 'platform-owner-projection-sources.ts', 'project-worker-startup-source-sql.ts', 'pp-read-tail-r44-sources.ts']) {
      expect(src(name), name + ' has no r45 symbol from another module').not.toMatch(name === 'pp-read-tail-r44-sources.ts' ? /ppRd45/ : /ppRd4[0-4][A-Za-z]+Projection\("SELECT/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1272', () => {
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
    expect(total).toBeLessThan(1272);
    expect(total).toBe(1196);
  });
});
