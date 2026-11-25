import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { ppRd64TimeEntryIdGuard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r64-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
};

// Byte oracles copied verbatim from the two pre-r64 inline call sites at HEAD,
// together with independent SHA-256 digests. The caller's predicate tail is
// passed through unchanged; the fixed head, closing ')' and bind order stay
// byte-identical. Both sites are read-only EXISTS assertion guards.
const head = 'EXISTS(SELECT 1 FROM hr_time_entries WHERE id=?';

type Row = {
  name: string;
  file: string;
  original: string;
  sha: string;
  binds: number;
  tail: string;
  call: string;
  inline: string;
  bindExpr: string;
};

// Binds are the original call-site expressions, in original order; ? count is checked.
const oracles: Row[] = [
  {
    name: 'payroll-readiness', file: 'payroll-readiness.ts',
    original: "EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND engagement_id=? AND version=? AND status='approved' AND work_date=? AND regular_hours=? AND overtime_hours=? AND cost_assignment_id IS NOT DISTINCT FROM ? AND cost_branch_id IS NOT DISTINCT FROM ? AND cost_department_id IS NOT DISTINCT FROM ?)",
    sha: 'bbd323a732eeec5a951d69e2d67e69b0fc27b7d2772b68afb0089796fa4215ff', binds: 9,
    tail: " AND engagement_id=? AND version=? AND status='approved' AND work_date=? AND regular_hours=? AND overtime_hours=? AND cost_assignment_id IS NOT DISTINCT FROM ? AND cost_branch_id IS NOT DISTINCT FROM ? AND cost_department_id IS NOT DISTINCT FROM ?",
    call: "ppRd64TimeEntryIdGuard(\" AND engagement_id=? AND version=? AND status='approved' AND work_date=? AND regular_hours=? AND overtime_hours=? AND cost_assignment_id IS NOT DISTINCT FROM ? AND cost_branch_id IS NOT DISTINCT FROM ? AND cost_department_id IS NOT DISTINCT FROM ?\")",
    inline: "\"EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND engagement_id=? AND version=? AND status='approved' AND work_date=? AND regular_hours=? AND overtime_hours=? AND cost_assignment_id IS NOT DISTINCT FROM ? AND cost_branch_id IS NOT DISTINCT FROM ? AND cost_department_id IS NOT DISTINCT FROM ?)\"",
    bindExpr: "ppRd64TimeEntryIdGuard(\" AND engagement_id=? AND version=? AND status='approved' AND work_date=? AND regular_hours=? AND overtime_hours=? AND cost_assignment_id IS NOT DISTINCT FROM ? AND cost_branch_id IS NOT DISTINCT FROM ? AND cost_department_id IS NOT DISTINCT FROM ?\"), [id, engagement_id, version, work_date, regular_hours, overtime_hours, cost_assignment_id, cost_branch_id, cost_department_id]",
  },
  {
    name: 'editable-time', file: 'people-payroll.ts',
    original: "EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?)",
    sha: '84a177b6a01f6c47a4dad4fbd549cc0cd9902ff2500568d76222cffeb7080738', binds: 3,
    tail: " AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?",
    call: "ppRd64TimeEntryIdGuard(\" AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?\")",
    inline: "\"EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?)\"",
    bindExpr: "ppRd64TimeEntryIdGuard(\" AND version=? AND status IN ('draft','submitted','rejected') AND submitted_by=?\"), [entryId, expectedVersion, userId]",
  },
];

describe('People Payroll read-tail r64 dedupe', () => {
  test('one owner-local builder renders two byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(2);
    for (const o of oracles) {
      const rendered = ppRd64TimeEntryIdGuard(o.tail);
      expect(rendered, o.name + ' byte oracle').toBe(o.original);
      expect(rendered, o.name + ' head+tail').toBe(head + o.tail + ')');
      expect(sha256(rendered), o.name + ' SHA oracle').toBe(o.sha);
      expect((rendered.match(/\?/g) ?? []).length, o.name + ' bind count').toBe(o.binds);
      readOnly(rendered);
    }
    expect(head).toBe('EXISTS(SELECT 1 FROM hr_time_entries WHERE id=?');
    expect(sha256(head)).toBe('cdc47906e53ab99fe4312d1a9fc859ccc81cb6648c6b62ea7007204ba0b96c2f');
    expect(oracles[0].original).not.toBe(oracles[1].original);
    expect(oracles[0].original).toContain(head);
    expect(oracles[1].original).toContain(head);
  });

  test('both callers delegate with unchanged binds and no inline copy', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r64 builder').toContain("from './pp-read-tail-r64-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.bindExpr));
      expect(src(o.file), o.file + ' no foreign r64 symbol').not.toMatch(/ppRd64(?!TimeEntryIdGuard)/);
    }
    expect(uses('payroll-readiness.ts', 'ppRd64TimeEntryIdGuard')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd64TimeEntryIdGuard')).toBe(1);
  });

  test('helper delegates its SELECT head to the shared round-2d builder', () => {
    const helper = src('pp-read-tail-r64-sources.ts');
    expect((helper.match(/export const ppRd64/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
    expect(helper).toContain("from './pp-read-tail-round2d-sources'");
    expect(helper).toContain("ppRdRound2dExistsById('hr_time_entries', tail)");
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(helper).not.toContain('@erpx/server-');
    // No differently-projected head or cross-owner tail was absorbed.
    expect(helper).not.toMatch(/FROM hr_people|FROM finance_expenses|FROM hr_engagements|expense_|finance_/);
  });

  test('r54-r63 builders, advisory, locks, native variants and other people reads stay untouched', () => {
    for (const [file, symbol] of [
      ['pp-read-tail-r54-sources.ts', 'ppRd54RunEvidenceGuard'],
      ['pp-read-tail-r55-sources.ts', 'ppRd55AttachmentPresenceV2'],
      ['pp-read-tail-r56-sources.ts', 'ppRd56EngagementGuard'],
      ['pp-read-tail-r57-sources.ts', 'ppRd57PeopleIdGuard'],
      ['pp-read-tail-r58-sources.ts', 'ppRd58RosterShiftsByWeekSql'],
      ['pp-read-tail-r59-sources.ts', 'ppRd59ThaiProfilesByCompanySql'],
      ['pp-read-tail-r60-sources.ts', 'ppRd60PayGroupsByCompanySql'],
      ['pp-read-tail-r61-sources.ts', 'ppRd61HolidayCalendarsByCompanySql'],
      ['pp-read-tail-r62-sources.ts', 'ppRd62PeopleEmailGuard'],
      ['pp-read-tail-r63-sources.ts', 'ppRd63CompRevisionEngagementGuard'],
    ] as const) {
      expect(src(file), file).toContain('export const ' + symbol);
    }
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('native_read_sql.rs')).toContain('SQL_PAYROLL_OPTIONS_GROUPS');
    // The r57 row-list and people-id heads now delegate to round-2c/2d; r31/r42 stay local.
    expect(src('pp-read-tail-r57-sources.ts')).toContain("ppRdRound2cStarByOnePred('hr_compensation_revisions', 'engagement_id', tail)");
    expect(src('pp-read-tail-r57-sources.ts')).toContain("ppRdRound2dExistsById('hr_people', tail)");
    expect(src('people-evaluation.ts')).toContain("ppRd31StatusGuard(false, 'hr_compensation_revisions', 'id', 'draft')");
    expect(src('people-payroll.ts')).toContain('ppRd42IdVersionLiteralGuard("hr_compensation_revisions","status")');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1195 (before 1196)', () => {
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
    expect(total).toBeLessThan(1196);
    expect(total).toBeLessThanOrEqual(1145);
  });
});
