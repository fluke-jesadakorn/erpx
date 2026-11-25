import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd59ThaiProfilesByCompanySql } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r59-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Byte oracles copied verbatim from the two pre-r59 inline call sites at HEAD.
// The caller's predicate/ORDER BY tail is passed through unchanged; the fixed
// head, projection, ? order and bind arity must stay byte-identical.
const head = 'SELECT * FROM hr_thai_payroll_profiles WHERE company_id=?';
const listAll = head + ' AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id';
const listApprovedByEmployee = head + " AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?";

// Independent verbatim copies of the original full literals, so the oracle does
// not rely solely on head+tail concatenation.
const originalListAll = 'SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id';
const originalListApprovedByEmployee = "SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?";

type Row = {
  file: string;
  render: () => string;
  literal: string;
  original: string;
  call: string;
  inline: string;
  binds: string;
};

// Binds are the original call-site expressions, in original order; ? count is checked.
const oracles: Row[] = [
  {
    file: 'payroll-statutory.ts', render: () => ppRd59ThaiProfilesByCompanySql(' AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id'),
    literal: listAll, original: originalListAll,
    call: 'ppRd59ThaiProfilesByCompanySql(" AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id")',
    inline: '"SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id"',
    binds: 'ppRd59ThaiProfilesByCompanySql(" AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id")).bind(companyId, value, value2).all()',
  },
  {
    file: 'payroll-statutory.ts', render: () => ppRd59ThaiProfilesByCompanySql(" AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?"),
    literal: listApprovedByEmployee, original: originalListApprovedByEmployee,
    call: `ppRd59ThaiProfilesByCompanySql(" AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?")`,
    inline: `"SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?"`,
    binds: `ppRd59ThaiProfilesByCompanySql(" AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?")).bind(company_id, engagementId, year, pay_date, pay_date2).all<Profile>()`,
  },
];

describe('People Payroll read-tail r59 dedupe', () => {
  test('one owner-local builder renders two byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(2);
    for (const o of oracles) {
      expect(o.render(), o.file + ' ' + o.call).toBe(o.literal);
      expect(o.render(), o.file + ' verbatim original literal').toBe(o.original);
      readOnly(o.literal);
    }
    // Bind arity is carried by the caller's ? list; the fixed head never shifts it.
    expect((listAll.match(/\?/g) ?? []).length).toBe(3);
    expect((listApprovedByEmployee.match(/\?/g) ?? []).length).toBe(5);
    // Tails stay distinct: each head renders only with its own original tail.
    expect(listAll).not.toBe(listApprovedByEmployee);
    expect(listAll).toContain(head + ' AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id');
    expect(listApprovedByEmployee).toContain(head + " AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?");
    expect(head).toBe('SELECT * FROM hr_thai_payroll_profiles WHERE company_id=?');
  });

  test('both callers delegate and keep no inline copy or bind change', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r59 builder').toContain("from './pp-read-tail-r59-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.binds));
    }
    expect(uses('payroll-statutory.ts', 'ppRd59ThaiProfilesByCompanySql')).toBe(2);
    expect(src('payroll-statutory.ts'), 'payroll-statutory has no foreign r59 symbol').not.toMatch(/ppRd59(?!ThaiProfilesByCompanySql)/);
  });

  test('helper owns exactly one SELECT head with no lock, native or cross-owner leakage', () => {
    const helper = src('pp-read-tail-r59-sources.ts');
    expect((helper.match(/export const ppRd59/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toContain('@erpx/server-');
    expect(helper).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(helper).not.toMatch(/payroll_run|hr_pay_groups|hr_engagements|expense_payments/);
  });

  test('r54-r58 builders, advisory, locks, native kinds and cross-owner tails stay untouched', () => {
    expect(src('pp-read-tail-r54-sources.ts')).toContain('export const ppRd54RunEvidenceGuard');
    expect(src('pp-read-tail-r55-sources.ts')).toContain('export const ppRd55AttachmentPresenceV2');
    expect(src('pp-read-tail-r56-sources.ts')).toContain('export const ppRd56EngagementGuard');
    expect(src('pp-read-tail-r57-sources.ts')).toContain('export const ppRd57PeopleIdGuard');
    expect(src('pp-read-tail-r58-sources.ts')).toContain('export const ppRd58RosterShiftsByWeekSql');
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    // The unique-payment-slip cross-owner projection tail remains inline at its caller.
    expect(src('expense-payments.ts')).toContain("used.owner_type='expense_payment' AND used.revoked_at IS NULL");
    expect(src('expense-payments.ts')).toContain("current.owner_type='expense_payment' AND current.owner_id=?");
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1200 (before 1201)', () => {
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
    expect(total).toBeLessThan(1201);
    expect(total).toBe(1164);
  });
});
