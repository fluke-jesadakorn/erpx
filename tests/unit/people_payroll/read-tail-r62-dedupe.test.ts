import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { ppRd62PeopleEmailGuard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r62-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
};

// Byte oracles copied verbatim from the two pre-r62 inline call sites at HEAD,
// together with independent SHA-256 digests. The caller's predicate tail is passed
// through unchanged; the fixed head, closing ')' and bind order stay byte-identical.
const head = 'NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=?';

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
    name: 'directory-email', file: 'people-directory.ts',
    original: 'NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=? AND id!=? AND merged_into_id IS NULL)',
    sha: '056e31a128bb6699fbb6482c394c5916f65d9704ddcd323039f38e784774f3bb', binds: 3,
    tail: ' AND lower(email)=? AND id!=? AND merged_into_id IS NULL',
    call: 'ppRd62PeopleEmailGuard(" AND lower(email)=? AND id!=? AND merged_into_id IS NULL")',
    inline: '"NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=? AND id!=? AND merged_into_id IS NULL)"',
    bindExpr: 'ppRd62PeopleEmailGuard(" AND lower(email)=? AND id!=? AND merged_into_id IS NULL"), [companyId, email, id]',
  },
  {
    name: 'hire-handoff-email', file: 'people-handoffs.ts',
    original: 'NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=lower(?) AND id<>?)',
    sha: 'bda734e7b9382a50aebfcd11be9cac3cf520a37690c8499568fdc3a9bdf3851f', binds: 3,
    tail: ' AND lower(email)=lower(?) AND id<>?',
    call: 'ppRd62PeopleEmailGuard(" AND lower(email)=lower(?) AND id<>?")',
    inline: '"NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=lower(?) AND id<>?)"',
    bindExpr: 'ppRd62PeopleEmailGuard(" AND lower(email)=lower(?) AND id<>?"), [companyId, email, personId]',
  },
];

describe('People Payroll read-tail r62 dedupe', () => {
  test('one owner-local builder renders two byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(2);
    for (const o of oracles) {
      const rendered = ppRd62PeopleEmailGuard(o.tail);
      expect(rendered, o.name + ' byte oracle').toBe(o.original);
      expect(rendered, o.name + ' head+tail').toBe(head + o.tail + ')');
      expect(sha256(rendered), o.name + ' SHA oracle').toBe(o.sha);
      expect((rendered.match(/\?/g) ?? []).length, o.name + ' bind count').toBe(o.binds);
      readOnly(rendered);
    }
    expect(head).toBe('NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=?');
    expect(sha256(head)).toBe('073887b64e7ac18779246f6be63684a38c86f3dc164e211ba2eefd0fc61934c3');
    expect(oracles[0].original).not.toBe(oracles[1].original);
    expect(oracles[0].original).toContain(head);
    expect(oracles[1].original).toContain(head);
  });

  test('both callers delegate with unchanged binds and no inline copy', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r62 builder').toContain("from './pp-read-tail-r62-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.bindExpr));
      expect(src(o.file), o.file + ' no foreign r62 symbol').not.toMatch(/ppRd62(?!PeopleEmailGuard)/);
    }
    expect(uses('people-directory.ts', 'ppRd62PeopleEmailGuard')).toBe(1);
    expect(uses('people-handoffs.ts', 'ppRd62PeopleEmailGuard')).toBe(1);
  });

  test('helper owns exactly one SELECT head with no write, lock, advisory or native text', () => {
    const helper = src('pp-read-tail-r62-sources.ts');
    expect((helper.match(/export const ppRd62/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).toContain('`NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=?${tail})`');
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(helper).not.toContain('@erpx/server-');
    // No differently-projected people head or cross-owner tail was absorbed.
    expect(helper).not.toMatch(/FROM hr_engagements|FROM finance_expenses|SELECT id|expense_|finance_/);
  });

  test('r54-r61 builders, advisory, locks, native variants and other people reads stay untouched', () => {
    for (const [file, symbol] of [
      ['pp-read-tail-r54-sources.ts', 'ppRd54RunEvidenceGuard'],
      ['pp-read-tail-r55-sources.ts', 'ppRd55AttachmentPresenceV2'],
      ['pp-read-tail-r56-sources.ts', 'ppRd56EngagementGuard'],
      ['pp-read-tail-r57-sources.ts', 'ppRd57PeopleIdGuard'],
      ['pp-read-tail-r58-sources.ts', 'ppRd58RosterShiftsByWeekSql'],
      ['pp-read-tail-r59-sources.ts', 'ppRd59ThaiProfilesByCompanySql'],
      ['pp-read-tail-r60-sources.ts', 'ppRd60PayGroupsByCompanySql'],
      ['pp-read-tail-r61-sources.ts', 'ppRd61HolidayCalendarsByCompanySql'],
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
    // The distinct people-id guard stays on its r57 builder, not this fold.
    expect(src('people-directory.ts')).toContain('ppRd57PeopleIdGuard(" AND version=? AND status!=\'archived\'")');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1197 (before 1198)', () => {
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
    expect(total).toBeLessThan(1198);
    expect(total).toBe(1196);
  });
});
