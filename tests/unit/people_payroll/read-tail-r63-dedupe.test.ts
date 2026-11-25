import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { ppRd63CompRevisionEngagementGuard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r63-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
};

// Byte oracles copied verbatim from the two pre-r63 inline call sites at HEAD,
// together with independent SHA-256 digests. The caller's predicate tail is
// passed through unchanged; the fixed head, closing ')' and bind order stay
// byte-identical. Both sites are read-only NO-EXISTS assertion guards.
const head = 'NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=?';

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
    name: 'no-other-wage', file: 'people-evaluation.ts',
    original: "NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id NOT IN (?,?) AND status='approved' AND (effective_to IS NULL OR effective_to>=?))",
    sha: '66b7b6b1b775363535a506b51b8948305904067a6d40d8b3cc96f5b3467668ee', binds: 4,
    tail: " AND id NOT IN (?,?) AND status='approved' AND (effective_to IS NULL OR effective_to>=?)",
    call: 'ppRd63CompRevisionEngagementGuard(" AND id NOT IN (?,?) AND status=\'approved\' AND (effective_to IS NULL OR effective_to>=?)")',
    inline: '"NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id NOT IN (?,?) AND status=\'approved\' AND (effective_to IS NULL OR effective_to>=?))"',
    bindExpr: 'ppRd63CompRevisionEngagementGuard(" AND id NOT IN (?,?) AND status=\'approved\' AND (effective_to IS NULL OR effective_to>=?)"), [id, value2, value3, effective]',
  },
  {
    name: 'overlap', file: 'people-payroll.ts',
    original: "NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id!=? AND status IN ('active','approved') AND effective_from<=COALESCE(?,'9999-12-31') AND COALESCE(effective_to,'9999-12-31')>=?)",
    sha: '5400a08cd8a9cd89d3d3275060e2114f6d4cba087b89a2c427a06593c0b0bd29', binds: 4,
    tail: " AND id!=? AND status IN ('active','approved') AND effective_from<=COALESCE(?,'9999-12-31') AND COALESCE(effective_to,'9999-12-31')>=?",
    call: 'ppRd63CompRevisionEngagementGuard(" AND id!=? AND status IN (\'active\',\'approved\') AND effective_from<=COALESCE(?,\'9999-12-31\') AND COALESCE(effective_to,\'9999-12-31\')>=?")',
    inline: '"NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id!=? AND status IN (\'active\',\'approved\') AND effective_from<=COALESCE(?,\'9999-12-31\') AND COALESCE(effective_to,\'9999-12-31\')>=?)"',
    bindExpr: 'ppRd63CompRevisionEngagementGuard(" AND id!=? AND status IN (\'active\',\'approved\') AND effective_from<=COALESCE(?,\'9999-12-31\') AND COALESCE(effective_to,\'9999-12-31\')>=?"), [engagement_id, compensationId, effective_to, effective_from]',
  },
];

describe('People Payroll read-tail r63 dedupe', () => {
  test('one owner-local builder renders two byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(2);
    for (const o of oracles) {
      const rendered = ppRd63CompRevisionEngagementGuard(o.tail);
      expect(rendered, o.name + ' byte oracle').toBe(o.original);
      expect(rendered, o.name + ' head+tail').toBe(head + o.tail + ')');
      expect(sha256(rendered), o.name + ' SHA oracle').toBe(o.sha);
      expect((rendered.match(/\?/g) ?? []).length, o.name + ' bind count').toBe(o.binds);
      readOnly(rendered);
    }
    expect(head).toBe('NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=?');
    expect(sha256(head)).toBe('fc4303f0c1fef39015089ad6914f4cf08ea7f076dd6cf159d8785f83965d0b56');
    expect(oracles[0].original).not.toBe(oracles[1].original);
    expect(oracles[0].original).toContain(head);
    expect(oracles[1].original).toContain(head);
  });

  test('both callers delegate with unchanged binds and no inline copy', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r63 builder').toContain("from './pp-read-tail-r63-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.bindExpr));
      expect(src(o.file), o.file + ' no foreign r63 symbol').not.toMatch(/ppRd63(?!CompRevisionEngagementGuard)/);
    }
    expect(uses('people-evaluation.ts', 'ppRd63CompRevisionEngagementGuard')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd63CompRevisionEngagementGuard')).toBe(1);
  });

  test('helper owns exactly one SELECT head with no write, lock, advisory or native text', () => {
    const helper = src('pp-read-tail-r63-sources.ts');
    expect((helper.match(/export const ppRd63/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).toContain('`NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=?${tail})`');
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(helper).not.toContain('@erpx/server-');
    // No differently-projected head or cross-owner tail was absorbed.
    expect(helper).not.toMatch(/FROM hr_people|FROM finance_expenses|FROM hr_engagements|expense_|finance_/);
  });

  test('r54-r62 builders, advisory, locks, native variants and other people reads stay untouched', () => {
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
    // The r57 SELECT * head and r31/r42 id-status guards remain on their builders.
    expect(src('pp-read-tail-r57-sources.ts')).toContain('SELECT * FROM hr_compensation_revisions WHERE engagement_id=?');
    expect(src('people-evaluation.ts')).toContain("ppRd31StatusGuard(false, 'hr_compensation_revisions', 'id', 'draft')");
    expect(src('people-payroll.ts')).toContain('ppRd42IdVersionLiteralGuard("hr_compensation_revisions","status")');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1196 (before 1197)', () => {
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
    expect(total).toBeLessThan(1197);
    expect(total).toBe(1196);
  });
});
