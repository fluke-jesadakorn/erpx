import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd52PaymentImage,
  ppRd52PaymentValidity,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r52-sources';
import {
  expensePaymentCancellations,
  expensePaymentTreasurySql,
} from '../../../packages/server-people_payroll/src/adapters/expense-payment-cancellation-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const flat = (text: string) => text.replace(/\s+/g, ' ');

// Sentinel fragments carry the exact bind arity of the former inline authority,
// native-source and independent tails. The Treasury/cancellation fragments are the
// real owner literals so the oracles prove byte identity, including their tails.
const AUTH = 'AUTH_?,?_SQL';
const NATIVE = 'NATIVE_?,?';
const INDEP = 'INDEP_?,?,?,?';

// Each literal is copied from the pre-r52 inline call site in canonical builder form.
// The four divergences are exactly: terminal EXISTS + Treasury tail, plain image,
// Treasury-inside-EXISTS authority tail, and native-source + independent tails.
const imageTerminal = `SELECT to_jsonb(d)::text draft,to_jsonb(e)::text expense FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id
    WHERE d.id=? AND d.expense_id=? AND d.company_id=? AND EXISTS(SELECT 1 FROM (${expensePaymentCancellations}) terminal WHERE terminal.id=d.id AND terminal.company_id=d.company_id)
    AND (${expensePaymentTreasurySql('e')})`;
const imagePlain = 'SELECT to_jsonb(d)::text draft,to_jsonb(e)::text expense FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id WHERE d.id=? AND d.expense_id=? AND d.company_id=?';
const validityMinimal = `SELECT (EXISTS(SELECT 1 FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id WHERE d.id=? AND d.company_id=? AND to_jsonb(d)=?::jsonb AND to_jsonb(e)=?::jsonb AND (${expensePaymentTreasurySql('e')})) AND (${AUTH})) ok`;
const validityFull = `SELECT (EXISTS(SELECT 1 FROM expense_payment_drafts d JOIN finance_expenses e ON e.id=d.expense_id AND e.company_id=d.company_id WHERE d.id=? AND d.company_id=? AND to_jsonb(d)=?::jsonb AND to_jsonb(e)=?::jsonb)
     AND (${NATIVE}) AND (${INDEP}) AND (${AUTH})) ok`;

const render = {
  ppRd52PaymentImage: (a: string[]) => ppRd52PaymentImage(a[0] === 'terminal'),
  ppRd52PaymentValidity: (a: string[]) => a[0] === 'minimal'
    ? ppRd52PaymentValidity(false, '', '', AUTH)
    : ppRd52PaymentValidity(true, NATIVE, INDEP, AUTH),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; binds: string[]; call: string; bindCall: string };

// One row per former inline site. Binds are the original arrays, in original order;
// spread binds are expanded to their exact arity so the ? count is checkable.
const oracles: Row[] = [
  {
    file: 'expense-payment-cancellation.ts', sym: 'ppRd52PaymentImage', args: ['terminal'], literal: imageTerminal,
    binds: ['id', 'expenseId', 'actor.companyId', 'actor.assignmentId', 'actor.userId'],
    call: 'ppRd52PaymentImage(true)', bindCall: '.bind(id,expenseId,actor.companyId,actor.assignmentId,actor.userId)',
  },
  {
    file: 'expense-payment-cancellation.ts', sym: 'ppRd52PaymentImage', args: ['plain'], literal: imagePlain,
    binds: ['id', 'expenseId', 'actor.companyId'],
    call: 'ppRd52PaymentImage(false)', bindCall: '.bind(id,expenseId,actor.companyId)',
  },
  {
    file: 'expense-payment-cancellation.ts', sym: 'ppRd52PaymentValidity', args: ['minimal'], literal: validityMinimal,
    binds: ['id', 'actor.companyId', 'current.draft', 'current.expense', 'actor.assignmentId', 'actor.userId', '...authority.args[0]', '...authority.args[1]'],
    call: "ppRd52PaymentValidity(false,'','',authority.query)", bindCall: '.bind(id,actor.companyId,current.draft,current.expense,actor.assignmentId,actor.userId,...authority.args)',
  },
  {
    file: 'expense-payment-cancellation.ts', sym: 'ppRd52PaymentValidity', args: ['full'], literal: validityFull,
    binds: ['id', 'actor.companyId', 'JSON.stringify(expectedDraft)', 'source.expense', 'id', 'actor.companyId', '...independentArgs[0]', '...independentArgs[1]', '...independentArgs[2]', '...independentArgs[3]', '...authority.args[0]', '...authority.args[1]'],
    call: 'ppRd52PaymentValidity(true,nativeSource,independent,authority.query)', bindCall: '.bind(id,actor.companyId,JSON.stringify(expectedDraft),source.expense,id,actor.companyId,...independentArgs,...authority.args)',
  },
];

describe('People Payroll read-tail r52 dedupe', () => {
  test('the two shared builders own byte-identical, read-only literals and bind counts', () => {
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.call).toBe(o.literal);
      expect((o.literal.match(/\?/g) ?? []).length, o.file + ' bind arity').toBe(o.binds.length);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(4);
    // The plain image keeps its original one-line WHERE tail for either terminal input.
    expect(ppRd52PaymentImage(false)).toBe(imagePlain);
    // No parameter may leak a lock, native kind or bind value.
    expect(src('pp-read-tail-r52-sources.ts')).not.toContain('nativePpStatement');
    expect((src('pp-read-tail-r52-sources.ts').match(/export const ppRd52/g) ?? []).length).toBe(2);
    expect((src('pp-read-tail-r52-sources.ts').match(/SELECT /g) ?? []).length).toBe(4);
  });

  test('each caller uses its builder, keeps no inline copy and keeps binds and spread', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.literal);
      expect(src(o.file), o.file + ' imports r52 builders').toContain("from './pp-read-tail-r52-sources'");
      expect(flat(src(o.file)), o.file + ' bind array').toContain(flat(o.bindCall));
    }
    expect(uses('expense-payment-cancellation.ts', 'ppRd52PaymentImage')).toBe(2);
    expect(uses('expense-payment-cancellation.ts', 'ppRd52PaymentValidity')).toBe(2);
    // The independent guard and native source stay at the consumer; the authority
    // fragment and spread args are still passed by the caller, not captured.
    expect(src('expense-payment-cancellation.ts')).toContain('const independent=ppRd29FinanceExpenseTreasurySql(expensePaymentTreasurySql(\'e\'));');
    expect(src('expense-payment-cancellation.ts')).toContain('const authority=await expenseAuthoritySnapshot(database,actor.companyId)');
  });

  test('locks, advisory, native kinds and r1-r51 builders stay untouched', () => {
    const helper = src('pp-read-tail-r52-sources.ts');
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
    expect(helper).toContain('expensePaymentCancellations');
    expect(helper).toContain('expensePaymentTreasurySql');
    expect(src('expense-payment-cancellation.ts')).toContain('SELECT id FROM finance_expenses WHERE id=? AND company_id=? FOR UPDATE');
    expect(src('expense-payment-cancellation.ts')).toContain('SELECT id FROM expense_payment_drafts WHERE id=? AND company_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT id FROM hr_payroll_runs WHERE id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('FOR SHARE');
    expect(src('people-evaluation.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('nativePpStatement');
    for (const name of ['expense-payment-cancellation.ts']) {
      expect(src(name), name + ' has no foreign r52 symbol').not.toMatch(/ppRd52(?!PaymentImage|PaymentValidity)/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1224', () => {
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
    expect(total).toBeLessThan(1224);
    expect(total).toBe(1164);
  });
});
