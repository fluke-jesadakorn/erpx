import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { ppRd61HolidayCalendarsByCompanySql } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r61-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
};

// Byte oracles copied verbatim from the two pre-r61 inline call sites at HEAD,
// together with independent SHA-256 digests. The caller's predicate/ORDER BY
// tail is passed through unchanged; the fixed head, projection, ? order and
// bind arity must stay byte-identical.
const head = 'SELECT id,name FROM leave_holiday_calendars WHERE company_id=?';
const activeOrdered = head + ' AND active=true ORDER BY name';
const plainOrdered = head + ' ORDER BY name';

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
    name: 'administration-calendars', file: 'leave-self-service.ts',
    original: 'SELECT id,name FROM leave_holiday_calendars WHERE company_id=? ORDER BY name',
    sha: '00e51f4303901635a463c78c8d8287ced65273074ffb55253e504e25d5d75abd', binds: 1,
    tail: ' ORDER BY name',
    call: "ppRd61HolidayCalendarsByCompanySql(' ORDER BY name')",
    inline: "'SELECT id,name FROM leave_holiday_calendars WHERE company_id=? ORDER BY name'",
    bindExpr: "ppRd61HolidayCalendarsByCompanySql(' ORDER BY name'), companyId);",
  },
  {
    name: 'hr-settings-calendars', file: 'people-hr-settings.ts',
    original: 'SELECT id,name FROM leave_holiday_calendars WHERE company_id=? AND active=true ORDER BY name',
    sha: 'f0d665b13d9435820a165bb53dd13264a6ea018af848c24f2b06657bfbe4024e', binds: 1,
    tail: ' AND active=true ORDER BY name',
    call: "ppRd61HolidayCalendarsByCompanySql(' AND active=true ORDER BY name')",
    inline: '"SELECT id,name FROM leave_holiday_calendars WHERE company_id=? AND active=true ORDER BY name"',
    bindExpr: "ppRd61HolidayCalendarsByCompanySql(' AND active=true ORDER BY name')).bind(companyId).all<{",
  },
];

describe('People Payroll read-tail r61 dedupe', () => {
  test('one owner-local builder renders two byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(2);
    for (const o of oracles) {
      const rendered = ppRd61HolidayCalendarsByCompanySql(o.tail);
      expect(rendered, o.name + ' byte oracle').toBe(o.original);
      expect(rendered, o.name + ' head+tail').toBe(head + o.tail);
      expect(sha256(rendered), o.name + ' SHA oracle').toBe(o.sha);
      expect((rendered.match(/\?/g) ?? []).length, o.name + ' bind count').toBe(o.binds);
      readOnly(rendered);
    }
    expect(head).toBe('SELECT id,name FROM leave_holiday_calendars WHERE company_id=?');
    expect(sha256(head)).toBe('191b7163c9d28fb593c67fc9feeb542b0f603a26d5f1d053afe95f6d16d67756');
    expect(plainOrdered).toBe(oracles[0].original);
    expect(activeOrdered).toBe(oracles[1].original);
    expect(plainOrdered).not.toBe(activeOrdered);
    expect(plainOrdered).toContain(head + ' ORDER BY name');
    expect(activeOrdered).toContain(head + ' AND active=true ORDER BY name');
  });

  test('both callers delegate with unchanged binds and no inline copy', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r61 builder').toContain("from './pp-read-tail-r61-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.bindExpr));
      expect(src(o.file), o.file + ' no foreign r61 symbol').not.toMatch(/ppRd61(?!HolidayCalendarsByCompanySql)/);
    }
    expect(uses('leave-self-service.ts', 'ppRd61HolidayCalendarsByCompanySql')).toBe(1);
    expect(uses('people-hr-settings.ts', 'ppRd61HolidayCalendarsByCompanySql')).toBe(1);
  });

  test('helper owns exactly one SELECT head with no write, lock, advisory or native text', () => {
    const helper = src('pp-read-tail-r61-sources.ts');
    expect((helper.match(/export const ppRd61/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).toContain('`SELECT id,name FROM leave_holiday_calendars WHERE company_id=?${tail}`');
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(helper).not.toContain('@erpx/server-');
    // No differently-projected calendar head or cross-owner tail was absorbed.
    expect(helper).not.toMatch(/SELECT 1 FROM leave_holiday_calendars|WHERE id=\?|expense_payments|hr_people|finance_/);
  });

  test('r54-r60 builders, advisory, locks, native variants and other calendar reads stay untouched', () => {
    for (const [file, symbol] of [
      ['pp-read-tail-r54-sources.ts', 'ppRd54RunEvidenceGuard'],
      ['pp-read-tail-r55-sources.ts', 'ppRd55AttachmentPresenceV2'],
      ['pp-read-tail-r56-sources.ts', 'ppRd56EngagementGuard'],
      ['pp-read-tail-r57-sources.ts', 'ppRd57PeopleIdGuard'],
      ['pp-read-tail-r58-sources.ts', 'ppRd58RosterShiftsByWeekSql'],
      ['pp-read-tail-r59-sources.ts', 'ppRd59ThaiProfilesByCompanySql'],
      ['pp-read-tail-r60-sources.ts', 'ppRd60PayGroupsByCompanySql'],
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
    // The distinct id+company calendar guard stays on its r36 builder, not this fold.
    expect(src('leave-self-service.ts')).toContain('ppRd36TwoPredTail("1","leave_holiday_calendars","id","company_id"," AND active=true")');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1198 (before 1199)', () => {
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
    expect(total).toBeLessThan(1199);
    expect(total).toBe(1167);
  });
});
