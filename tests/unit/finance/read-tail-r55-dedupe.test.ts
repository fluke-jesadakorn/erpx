import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { companyCountSql, correlatedCountSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r55-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const q = (s: string) => (s.includes("'") ? `"${s.replaceAll('"', '\\"')}"` : `'${s}'`);
const call = (cs: Case) => `${cs.helper}(${cs.args.map(q).join(', ')})`;
const ws = 'finance-workspace.ts';
const ir = 'finance-invoice-read.ts';

type Case = {
  file: string;
  helper: 'companyCountSql' | 'correlatedCountSql';
  args: string[];
  expected: string;
  label: string;
};

const c = (file: string, table: string, tail: string, label: string): Case => ({
  file, helper: 'companyCountSql', args: [table, tail],
  expected: `SELECT count(*) FROM ${table} WHERE company_id=?${tail}`, label,
});
const p = (alias: string, table: string, tail: string, label: string): Case => ({
  file: ws, helper: 'correlatedCountSql', args: [alias, table, tail],
  expected: `SELECT count(*) FROM ${table} ${alias} WHERE ${alias}.company_id=p.company_id${tail}`, label,
});

const CASES: Case[] = [
  c(ws, 'finance_invoices', " AND status='draft'", 'draft_invoices'),
  c(ws, 'finance_invoices', " AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<?", 'overdue_receivables'),
  c(ws, 'finance_invoices', " AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<=?", 'due_payables'),
  c(ws, 'finance_journals', " AND status IN ('draft','in_review')", 'draft_journals'),
  c(ws, 'finance_payments', " AND status NOT IN ('settled','voided') AND outstanding_minor>0", 'pending_payments/1'),
  c(ws, 'finance_refund_obligations', " AND status IN ('pending','approved') AND outstanding_minor>0", 'pending_payments/3'),
  c(ws, 'finance_periods', " AND state='closing'", 'closing_periods'),
  c(ws, 'finance_periods', " AND state='open' AND end_date<?", 'expired_open_periods'),
  p('j', 'finance_journals', " AND j.period_id=p.id AND j.status IN ('draft','in_review')", 'period draft_journals'),
  p('b', 'finance_bank_transactions', " AND b.transaction_date BETWEEN p.start_date AND p.end_date AND b.match_status<>'matched'", 'period unmatched_bank'),
  p('i', 'finance_invoices', " AND i.status IN ('draft','in_review') AND i.invoice_date BETWEEN p.start_date AND p.end_date", 'period draft_invoices'),
  p('r', 'finance_calculation_runs', " AND r.period_id=p.id AND r.status IN ('prepared','reviewed','stale')", 'period pending_calculations'),
  c(ir, 'finance_journals', ' ', 'AS journals'),
  c(ir, 'finance_journals', " AND status='draft'", 'AS posting'),
  c(ir, 'finance_invoices', '', 'AS invoices'),
  c(ir, 'finance_payments', '', 'AS payments'),
  c(ir, 'finance_invoices', " AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0", 'AS receivable'),
  c(ir, 'finance_invoices', " AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0", 'AS payable'),
];

const render = (cs: Case) => cs.helper === 'companyCountSql'
  ? companyCountSql(cs.args[0], cs.args[1])
  : correlatedCountSql(cs.args[0], cs.args[1], cs.args[2]);

// Reconstructed independently of the helper bodies: the closed byte oracle.
const oracle = (cs: Case) => cs.helper === 'companyCountSql'
  ? `SELECT count(*) FROM ${cs.args[0]} WHERE company_id=?${cs.args[1]}`
  : `SELECT count(*) FROM ${cs.args[1]} ${cs.args[0]} WHERE ${cs.args[0]}.company_id=p.company_id${cs.args[2]}`;

describe('Finance read-tail r55 closed company count helpers', () => {
  test('all 18 sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(18);
    for (const cs of CASES) {
      expect(render(cs)).toBe(cs.expected);
      expect(render(cs)).toBe(oracle(cs));
      expect(count(render(cs), 'SELECT ')).toBe(1);
      expect(render(cs).startsWith('SELECT count(*) FROM ')).toBe(true);
    }
    // The trailing-space tail for the `journals` head is deliberate.
    expect(CASES[12].args[1]).toBe(' ');
    expect(companyCountSql('finance_journals', '')).not.toBe(correlatedCountSql('j', 'finance_journals', ''));
    expect(companyCountSql('finance_invoices', ' ')).not.toBe(companyCountSql('finance_journals', ' '));
  });

  test('callers delegate with unchanged tails, output labels, due_date predicates and binds', () => {
    for (const cs of CASES) {
      const src = read(cs.file);
      expect(norm(src)).toContain(norm(call(cs)));
      expect(src).not.toContain(`'${cs.expected}'`);
      expect(src).not.toContain(`"${cs.expected}"`);
    }
    const workspace = read(ws);
    const invoice = read(ir);
    expect(norm(workspace)).toContain(norm("import { companyCountSql, correlatedCountSql } from './finance-read-tail-r55-sql';"));
    expect(norm(invoice)).toContain(norm("import { companyCountSql } from './finance-read-tail-r55-sql';"));
    // workbenchCounters retains the interleaved company/today bind order.
    expect(norm(workspace)).toContain(norm(`.bind(
      actor.companyId,
      actor.companyId,
      today,
      actor.companyId,
      today,
      actor.companyId,
      today,
      actor.companyId,
      today,
      actor.companyId,`));
    expect(norm(workspace)).toContain('due_date<?');
    expect(norm(workspace)).toContain('due_date<=?');
    expect(norm(workspace)).toContain('.bind(actor.companyId)');
    // invoice overview keeps its seven company binds and output aliases.
    expect(norm(invoice)).toContain('.bind(...Array(7).fill(actor.companyId))');
    for (const label of ['journals', 'posting', 'invoices', 'payments', 'receivable', 'payable', 'reconcile']) {
      expect(invoice).toContain(` AS ${label}`);
    }
  });

  test('delegation counts match the 8 company + 4 correlated + 6 invoice anchors', () => {
    expect(count(read(ws), 'companyCountSql(')).toBe(8);
    expect(count(read(ws), 'correlatedCountSql(')).toBe(4);
    expect(count(read(ir), 'companyCountSql(')).toBe(6);
  });
});

describe('Finance read-tail r55 protected families and retirement', () => {
  test('sums, aliased/JOIN/derived-table counts, pending/AI heads and :123-129 stay inline', () => {
    const workspace = read(ws);
    expect(workspace).toContain("(SELECT COALESCE(sum(outstanding_minor),0) FROM finance_invoices WHERE company_id=? AND kind='receivable'");
    expect(workspace).toContain("(SELECT COALESCE(sum(outstanding_minor),0) FROM finance_invoices WHERE company_id=? AND kind='payable'");
    expect(workspace).toContain('(SELECT count(*) FROM finance_supplier_batches b WHERE b.company_id=?');
    expect(workspace).toContain('EXISTS (SELECT 1 FROM finance_supplier_instructions i WHERE i.batch_id=b.id');
    expect(workspace).toContain('${financeWorkspacePendingPeopleSql}');
    expect(workspace).toContain('${UNMATCHED_BANK_TXN_COUNT_SQL}');
    expect(workspace).toContain('${financeWorkspaceReadyExpenseAiSql}');
    // :123-129: output alias `count`, count(DISTINCT), JOIN and derived table are
    // not the closed company-first template and remain byte-for-byte inline.
    expect(workspace).toContain('"SELECT count(*) count FROM finance_accounts WHERE company_id=? AND status=\'active\'"');
    expect(workspace).toContain('"SELECT count(*) count FROM finance_periods WHERE company_id=?"');
    expect(workspace).toContain('"SELECT count(DISTINCT bank_account) count FROM finance_bank_statements WHERE company_id=?"');
    expect(workspace).toContain('"SELECT count(*) count FROM finance_accounting_profiles p JOIN finance_charts c');
    expect(workspace).toContain('"SELECT count(*) count FROM (SELECT a.chart_id FROM finance_accounts a');
    expect(read(ir)).toContain('${UNMATCHED_BANK_TXN_COUNT_SQL}');
  });

  test('prior helpers stay imported and each r55 helper is defined exactly once', () => {
    const helper = read('finance-read-tail-r55-sql.ts');
    expect(count(helper, 'export const companyCountSql =')).toBe(1);
    expect(count(helper, 'export const correlatedCountSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(2);
    expect(read(ws)).toContain("import { financeReferenceListSql } from './finance-read-tail-r42-sql';");
    expect(read(ws)).toContain("import { UNMATCHED_BANK_TXN_COUNT_SQL } from './finance-read-tail-r44-sql';");
    expect(read(ir)).toContain("import { UNMATCHED_BANK_TXN_COUNT_SQL } from './finance-read-tail-r44-sql';");
  });

  test('retirement:ts-statements pins the r59 target of 1707', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 18 count literals collapse into two helpers (-18 literals, +2 declarations: 1742 -> 1726).
    expect(CASES).toHaveLength(18);
    expect(total).toBe(1707);
    expect(total).toBeLessThan(1742);
  });
});
