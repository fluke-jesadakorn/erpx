import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd58RosterShiftsByWeekSql } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r58-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Byte oracles copied verbatim from the two pre-r58 inline call sites at HEAD.
// The caller's state/ORDER BY tail is passed through unchanged; the fixed head,
// projection, ? order and bind arity must stay byte-identical.
const shiftHead = 'SELECT * FROM people_roster_shifts WHERE week_id=?';
const activeOrdered = shiftHead + " AND state='active' ORDER BY starts_at,id";
const active = shiftHead + " AND state='active'";

type Row = {
  file: string;
  render: () => string;
  literal: string;
  call: string;
  inline: string;
  binds: string;
};

// Binds are the original call-site arrays/tuples, in original order; ? count is checked.
const oracles: Row[] = [
  {
    file: 'people-roster.ts', render: () => ppRd58RosterShiftsByWeekSql(" AND state='active' ORDER BY starts_at,id"),
    literal: activeOrdered, call: 'ppRd58RosterShiftsByWeekSql(" AND state=\'active\' ORDER BY starts_at,id")',
    inline: '"SELECT * FROM people_roster_shifts WHERE week_id=? AND state=\'active\' ORDER BY starts_at,id"',
    binds: 'ppRd58RosterShiftsByWeekSql(" AND state=\'active\' ORDER BY starts_at,id")).bind(fact0).all<RosterShift>()',
  },
  {
    file: 'people-roster.ts', render: () => ppRd58RosterShiftsByWeekSql(" AND state='active'"),
    literal: active, call: 'ppRd58RosterShiftsByWeekSql(" AND state=\'active\'")',
    inline: '"SELECT * FROM people_roster_shifts WHERE week_id=? AND state=\'active\'"',
    binds: 'ppRd58RosterShiftsByWeekSql(" AND state=\'active\'")).bind(fact0).all<RosterShift>()',
  },
];

describe('People Payroll read-tail r58 dedupe', () => {
  test('one owner-local builder renders two byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(2);
    for (const o of oracles) {
      expect(o.render(), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
    }
    // Bind arity is carried by the caller's ? list; the fixed head never shifts it.
    expect((activeOrdered.match(/\?/g) ?? []).length).toBe(1);
    expect((active.match(/\?/g) ?? []).length).toBe(1);
    // Tails stay distinct: each head renders only with its own original tail.
    expect(activeOrdered).not.toBe(active);
    expect(activeOrdered).toContain(shiftHead + " AND state='active' ORDER BY starts_at,id");
    expect(active).toContain(shiftHead);
    expect(shiftHead).toBe('SELECT * FROM people_roster_shifts WHERE week_id=?');
  });

  test('both callers delegate and keep no inline copy or bind change', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r58 builder').toContain("from './pp-read-tail-r58-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.binds));
    }
    expect(uses('people-roster.ts', 'ppRd58RosterShiftsByWeekSql')).toBe(2);
    expect(src('people-roster.ts'), 'people-roster has no foreign r58 symbol').not.toMatch(/ppRd58(?!RosterShiftsByWeekSql)/);
  });

  test('helper owns exactly one SELECT head with no lock, native or cross-owner leakage', () => {
    const helper = src('pp-read-tail-r58-sources.ts');
    expect((helper.match(/export const ppRd58/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toContain('@erpx/server-');
    expect(helper).not.toMatch(/financeProjectionSources|commerceProjectionSources|identity_accessProjectionSources|workflowProjectionSources/);
  });

  test('r54/r55/r56/r57 builders, advisory, locks, native kinds and cross-owner tails stay untouched', () => {
    expect(src('pp-read-tail-r54-sources.ts')).toContain('export const ppRd54RunEvidenceGuard');
    expect(src('pp-read-tail-r55-sources.ts')).toContain('export const ppRd55AttachmentPresenceV2');
    expect(src('pp-read-tail-r56-sources.ts')).toContain('export const ppRd56EngagementGuard');
    expect(src('pp-read-tail-r57-sources.ts')).toContain('export const ppRd57PeopleIdGuard');
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

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1201 (before 1202)', () => {
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
    expect(total).toBeLessThan(1202);
    expect(total).toBe(1164);
  });
});
