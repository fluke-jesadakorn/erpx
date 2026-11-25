import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd56ExpenseClaimStatusGuard,
  ppRd56EngagementGuard,
  ppRd56PayrollItemRow,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r56-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Byte oracles copied verbatim from the seven pre-r56 inline call sites at HEAD.
// The caller's status list / predicate tail is passed through unchanged; the
// fixed head, owner kind, ? order and bind arity must stay byte-identical.
const expenseLiteral = "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected','in_review'))";
const expenseStates = (states: string) => `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN (${states}))`;
const engVersionStatus = "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status IN ('active','ended'))";
const engPayGroup = "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=? AND worker_type='employee' AND version=? AND status IN ('active','ended'))";
const engBranch = "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))";
const itemHead = 'SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND ';
const itemPosted = itemHead + "net_minor>0 AND status='posted' ORDER BY engagement_id";
const itemRef = itemHead + "status='posted' AND ?=('PAY-'||?||'-'||id)";

type Row = {
  file: string;
  render: () => string;
  literal: string;
  call: string;
  inline: string;
  binds: string;
};

const oracles: Row[] = [
  {
    file: 'expense-workflow.ts', render: () => ppRd56ExpenseClaimStatusGuard("'draft','rejected','in_review'"),
    literal: expenseLiteral, call: 'ppRd56ExpenseClaimStatusGuard("\'draft\',\'rejected\',\'in_review\'")',
    inline: "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected','in_review'))",
    binds: '[id, actor.companyId, input.expectedVersion]',
  },
  {
    file: 'expenses.ts', render: () => ppRd56ExpenseClaimStatusGuard('?,?'),
    literal: expenseStates('?,?'), call: 'ppRd56ExpenseClaimStatusGuard(states.map(() => "?").join(","))',
    inline: 'EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN (${states',
    binds: '[id, actor.companyId, version, ...states]',
  },
  {
    file: 'payroll-adjustments.ts', render: () => ppRd56EngagementGuard("version=? AND status IN ('active','ended')"),
    literal: engVersionStatus, call: `ppRd56EngagementGuard("version=? AND status IN ('active','ended')")`,
    inline: "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status IN ('active','ended'))",
    binds: '[value2, company_id, value3]',
  },
  {
    file: 'payroll-readiness.ts', render: () => ppRd56EngagementGuard("pay_group_id=? AND currency=? AND worker_type='employee' AND version=? AND status IN ('active','ended')"),
    literal: engPayGroup, call: `ppRd56EngagementGuard("pay_group_id=? AND currency=? AND worker_type='employee' AND version=? AND status IN ('active','ended')")`,
    inline: "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=? AND worker_type='employee' AND version=? AND status IN ('active','ended'))",
    binds: '[engagement_id, company_id, pay_group_id, currency, version]',
  },
  {
    file: 'people-payroll.ts', render: () => ppRd56EngagementGuard('version=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)'),
    literal: engBranch, call: 'ppRd56EngagementGuard("version=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)")',
    inline: 'EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?))',
    binds: '[engagementId, companyId, expectedVersion, branchId, branchId2, departmentId, departmentId2]',
  },
  {
    file: 'people-payroll.ts', render: () => ppRd56PayrollItemRow("net_minor>0 AND status='posted' ORDER BY engagement_id"),
    literal: itemPosted, call: `ppRd56PayrollItemRow("net_minor>0 AND status='posted' ORDER BY engagement_id")`,
    inline: 'SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND net_minor>0',
    binds: '.bind(sourceId).all<{',
  },
  {
    file: 'people-payroll.ts', render: () => ppRd56PayrollItemRow("status='posted' AND ?=('PAY-'||?||'-'||id)"),
    literal: itemRef, call: `ppRd56PayrollItemRow("status='posted' AND ?=('PAY-'||?||'-'||id)")`,
    inline: 'SELECT id,engagement_id,net_minor,currency,input_snapshot_json FROM hr_payroll_items WHERE run_id=? AND status=\'posted\'',
    binds: '.bind(source_id, reference, source_id2).first<{',
  },
];

describe('People Payroll read-tail r56 dedupe', () => {
  test('three owner-local builders render seven byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(7);
    for (const o of oracles) {
      expect(o.render(), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
    }
    // Bind arity is carried by the caller's ? list; the fixed head never shifts it.
    expect((expenseLiteral.match(/\?/g) ?? []).length).toBe(3);
    expect((expenseStates('?,?').match(/\?/g) ?? []).length).toBe(5);
    expect((engVersionStatus.match(/\?/g) ?? []).length).toBe(3);
    expect((engPayGroup.match(/\?/g) ?? []).length).toBe(5);
    expect((engBranch.match(/\?/g) ?? []).length).toBe(7);
    expect((itemPosted.match(/\?/g) ?? []).length).toBe(1);
    expect((itemRef.match(/\?/g) ?? []).length).toBe(3);
    // Tails stay distinct: each head renders only with its own original tail.
    expect(itemPosted).not.toBe(itemRef);
    expect(engPayGroup).not.toBe(engVersionStatus);
    expect(expenseStates("'draft','rejected','in_review'")).toBe(expenseLiteral);
  });

  test('all seven callers delegate and keep no inline copy or bind change', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r56 builders').toContain("from './pp-read-tail-r56-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.binds));
    }
    expect(uses('expense-workflow.ts', 'ppRd56ExpenseClaimStatusGuard')).toBe(1);
    expect(uses('expenses.ts', 'ppRd56ExpenseClaimStatusGuard')).toBe(1);
    expect(uses('payroll-adjustments.ts', 'ppRd56EngagementGuard')).toBe(1);
    expect(uses('payroll-readiness.ts', 'ppRd56EngagementGuard')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd56EngagementGuard')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd56PayrollItemRow')).toBe(2);
    // The caller still owns the status-placeholder construction.
    expect(src('expenses.ts')).toContain('states.map(() => "?").join(",")');
  });

  test('helper owns exactly three SELECT heads with no lock, native or projection leakage', () => {
    const helper = src('pp-read-tail-r56-sources.ts');
    expect((helper.match(/export const ppRd56/g) ?? []).length).toBe(3);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(3);
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toContain('@erpx/server-');
  });

  test('r54/r55 builders, native kinds, locks and cross-owner tails stay untouched', () => {
    expect(src('pp-read-tail-r54-sources.ts')).toContain('export const ppRd54RunEvidenceGuard');
    expect(src('pp-read-tail-r55-sources.ts')).toContain('export const ppRd55AttachmentPresenceV2');
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    // The unique-payment-slip cross-owner projection tail remains inline at its caller.
    expect(src('expense-payments.ts')).toContain("used.owner_type='expense_payment' AND used.revoked_at IS NULL");
    expect(src('expense-payments.ts')).toContain("current.owner_type='expense_payment' AND current.owner_id=?");
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1206 (before 1210)', () => {
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
    expect(total).toBeLessThan(1210);
    expect(total).toBe(1197);
  });
});
