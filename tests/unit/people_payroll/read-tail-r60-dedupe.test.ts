import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { ppRd60PayGroupsByCompanySql } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r60-sources';

const src = (name: string) => readFileSync(new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
};

// Byte oracles copied verbatim from the two pre-r60 inline call sites at HEAD,
// together with independent SHA-256 digests. The caller's predicate/ORDER BY
// tail is passed through unchanged; the fixed head, projection, ? order and
// bind arity must stay byte-identical.
const head = 'SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=?';
const statusOrdered = head + " AND status='active' ORDER BY name,id";
const currencyStatusOrdered = head + " AND currency=? AND status='active' ORDER BY name,id";

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
    name: 'entrypoint', file: 'people-app-entrypoint-facts.ts',
    original: "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status='active' ORDER BY name,id",
    sha: '91cec317162e4c834176fffb5fff4feeafa1f4371b980e252ec4200c618e1563', binds: 1,
    tail: " AND status='active' ORDER BY name,id",
    call: `ppRd60PayGroupsByCompanySql(" AND status='active' ORDER BY name,id")`,
    inline: `"SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status='active' ORDER BY name,id"`,
    bindExpr: `ppRd60PayGroupsByCompanySql(" AND status='active' ORDER BY name,id")).bind(companyId).all()`,
  },
  {
    name: 'handoff', file: 'hire-handoff-workspace.ts',
    original: "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND currency=? AND status='active' ORDER BY name,id",
    sha: '8e33dc1b9e5ebc799d9585c464504363efe65a9f996abc80b8f46fbd9c347d31', binds: 2,
    tail: " AND currency=? AND status='active' ORDER BY name,id",
    call: `ppRd60PayGroupsByCompanySql(" AND currency=? AND status='active' ORDER BY name,id")`,
    inline: `"SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND currency=? AND status='active' ORDER BY name,id"`,
    bindExpr: `ppRd60PayGroupsByCompanySql(" AND currency=? AND status='active' ORDER BY name,id")).bind(actor.companyId,source.currency).all<{id:string;name:string;currency:string;frequency:string}>()`,
  },
];

describe('People Payroll read-tail r60 dedupe', () => {
  test('one owner-local builder renders two byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(2);
    for (const o of oracles.filter(o => o.file !== 'people-app-entrypoint-facts.ts')) {
      const rendered = ppRd60PayGroupsByCompanySql(o.tail);
      expect(rendered, o.name + ' byte oracle').toBe(o.original);
      expect(rendered, o.name + ' head+tail').toBe(head + o.tail);
      expect(sha256(rendered), o.name + ' SHA oracle').toBe(o.sha);
      expect((rendered.match(/\?/g) ?? []).length, o.name + ' bind count').toBe(o.binds);
      readOnly(rendered);
    }
    expect(head).toBe('SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=?');
    expect(sha256(head)).toBe('e820ae70dc8145c63af6153c53a143d7f0503f00b45ba1c21fba07f1b383c151');
    expect(statusOrdered).toBe(oracles[0].original);
    expect(currencyStatusOrdered).toBe(oracles[1].original);
    expect(statusOrdered).not.toBe(currencyStatusOrdered);
    expect(statusOrdered).toContain(head + " AND status='active' ORDER BY name,id");
    expect(currencyStatusOrdered).toContain(head + " AND currency=? AND status='active' ORDER BY name,id");
  });

  test('both callers delegate with unchanged binds and no inline copy', () => {
    for (const o of oracles.filter(o => o.file !== 'people-app-entrypoint-facts.ts')) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r60 builder').toContain("from './pp-read-tail-r60-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.bindExpr));
      expect(src(o.file), o.file + ' no foreign r60 symbol').not.toMatch(/ppRd60(?!PayGroupsByCompanySql)/);
    }
    expect(uses('people-app-entrypoint-facts.ts', 'ppRd60PayGroupsByCompanySql')).toBe(0);
    expect(uses('hire-handoff-workspace.ts', 'ppRd60PayGroupsByCompanySql')).toBe(1);
  });

  test('helper owns exactly one SELECT head with no write, lock, advisory or native text', () => {
    const helper = src('pp-read-tail-r60-sources.ts');
    expect((helper.match(/export const ppRd60/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).toContain('`SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=?${tail}`');
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement|SQL_PAYROLL_OPTIONS_GROUPS/i);
    expect(helper).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(helper).not.toContain('@erpx/server-');
    // No cross-owner or other-family tail was absorbed into the head.
    expect(helper).not.toMatch(/hr_people|hr_thai_payroll_profiles|talent_hire_consumption_receipts|expense_payments|finance_/);
  });

  test('r34-r59 builders, advisory, locks, native variants and other pay-group reads stay untouched', () => {
    for (const [file, symbol] of [
      ['pp-read-tail-r34-sources.ts', 'ppRd34ActivePayGroupProjection'],
      ['pp-read-tail-r54-sources.ts', 'ppRd54RunEvidenceGuard'],
      ['pp-read-tail-r55-sources.ts', 'ppRd55AttachmentPresenceV2'],
      ['pp-read-tail-r56-sources.ts', 'ppRd56EngagementGuard'],
      ['pp-read-tail-r57-sources.ts', 'ppRd57PeopleIdGuard'],
      ['pp-read-tail-r58-sources.ts', 'ppRd58RosterShiftsByWeekSql'],
      ['pp-read-tail-r59-sources.ts', 'ppRd59ThaiProfilesByCompanySql'],
    ] as const) {
      expect(src(file), file).toContain('export const ' + symbol);
    }
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    // Native and differently-projected pay-group reads are excluded from the fold.
    expect(src('native_read_sql.rs')).toContain('SQL_PAYROLL_OPTIONS_GROUPS: &str = "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND status=\'active\' ORDER BY name"');
    expect(src('hire-handoff-pay-group.ts')).toContain('"SELECT id FROM hr_pay_groups WHERE company_id=? AND currency=? AND status=\'active\' ORDER BY id"');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1199 (before 1200)', () => {
    const root = new URL('../../../modules/people-payroll/server/src/', import.meta.url);
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
    expect(total).toBeLessThan(1200);
    expect(total).toBe(1164);
  });
});
