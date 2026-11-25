import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { bankMatchGuardSql, companyIdRowsSql, receiptIdByPaymentSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r56-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const BANK_HEAD = 'EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=?';

type Helper = 'bankMatchGuardSql' | 'companyIdRowsSql' | 'receiptIdByPaymentSql';
type Case = { file: string; helper: Helper; args: string[]; call: string; expected: string; sha: string; label: string };

const CASES: Case[] = [
  { file: "customer-receipt-issue.ts", helper: 'receiptIdByPaymentSql', args: [" AND company_id=?"],
    call: "receiptIdByPaymentSql(' AND company_id=?')", expected: "SELECT id FROM finance_customer_receipts WHERE payment_id=? AND company_id=?",
    sha: '61b75c825b0611bcf0fa458b401283002a8922364df6c89131bf9cc1474a5ee8', label: "existing receipt / company" },
  { file: "customer-receipt-issue.ts", helper: 'receiptIdByPaymentSql', args: [""],
    call: "receiptIdByPaymentSql('')", expected: "SELECT id FROM finance_customer_receipts WHERE payment_id=?",
    sha: 'ace032f408e0c210781909397f303bffd453082219037b08226cf09d1ef76b68', label: "post-lock receipt" },
  { file: "company-money.ts", helper: 'companyIdRowsSql', args: ["currency,scale", "company_currency_scales", " AND currency IN (?,?)"],
    call: "companyIdRowsSql('currency,scale','company_currency_scales',` AND currency IN (${currencies.map(() => '?').join(',')})`)", expected: "SELECT currency,scale FROM company_currency_scales WHERE company_id=? AND currency IN (?,?)",
    sha: '90a8349e19025adab4b2d26c81dafdc9d280a3f893cecf3320909107f2d38675', label: "currency scales / in-list" },
  { file: "company-money.ts", helper: 'companyIdRowsSql', args: ["currency,scale", "company_currency_scales", ""],
    call: "companyIdRowsSql('currency,scale','company_currency_scales','')", expected: "SELECT currency,scale FROM company_currency_scales WHERE company_id=?",
    sha: '69d07c9462e921d1503f3bd7f58d69d3adb8dab0ef905010493374c8e8610a83', label: "currency scales / point" },
  { file: "finance-withholding-remittance.ts", helper: 'bankMatchGuardSql', args: [" AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.external_id=? AND s.bank_account=?"],
    call: "bankMatchGuardSql(\" AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.external_id=? AND s.bank_account=?\")", expected: "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.external_id=? AND s.bank_account=?)",
    sha: 'd351819ce7eed287c8aa9795c2b6104cbb31c7eab4133d647027697be0cc0170', label: "withholding monthly bank" },
  { file: "refunds.ts", helper: 'bankMatchGuardSql', args: [" AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.currency=? AND s.bank_account=?"],
    call: "bankMatchGuardSql(\" AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.currency=? AND s.bank_account=?\")", expected: "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.currency=? AND s.bank_account=?)",
    sha: '982eb59cc3b9e680732c22297a9b1e5516862a9a3a105053f16724864dcca346', label: "refund bank" },
  { file: "finance.ts", helper: 'bankMatchGuardSql', args: [" AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.transaction_date>=?"],
    call: "bankMatchGuardSql(\" AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.transaction_date>=?\")", expected: "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.transaction_date>=?)",
    sha: 'a5ca21053d9afcefb67f2693c6f40011b3865f22a95fa3dd53bded86c66b540a', label: "payment bank" },
  { file: "finance-assets.ts", helper: 'companyIdRowsSql', args: ["id,name,start_date,end_date,state", "finance_periods", " ORDER BY end_date DESC,id"],
    call: "companyIdRowsSql('id,name,start_date,end_date,state','finance_periods',' ORDER BY end_date DESC,id')", expected: "SELECT id,name,start_date,end_date,state FROM finance_periods WHERE company_id=? ORDER BY end_date DESC,id",
    sha: 'cc869f6e6067d6f47eeed00816970de36c7155af433b2d44b011dfc960bc45c2', label: "asset period options" },
  { file: "finance-engines.ts", helper: 'companyIdRowsSql', args: ["id,name,start_date,end_date,state", "finance_periods", " ORDER BY start_date DESC LIMIT 24"],
    call: "companyIdRowsSql('id,name,start_date,end_date,state','finance_periods',' ORDER BY start_date DESC LIMIT 24')", expected: "SELECT id,name,start_date,end_date,state FROM finance_periods WHERE company_id=? ORDER BY start_date DESC LIMIT 24",
    sha: '5a8b3cd8da2ebdee37753259bcdcdd88ae349f959c9bd532d6e0e6ff145d375f', label: "engine period list" },
  { file: "finance-assets.ts", helper: 'companyIdRowsSql', args: ["id,category", "finance_accounts", " AND chart_id=? AND status='active' AND id IN (?,?,?)"],
    call: "companyIdRowsSql('id,category','finance_accounts',\" AND chart_id=? AND status='active' AND id IN (?,?,?)\")", expected: "SELECT id,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (?,?,?)",
    sha: '5726c087471ae4cbe9ad05330f152a495dbdc9b5b7e3ed8103582adca4315edd', label: "asset disposal accounts" },
  { file: "finance.ts", helper: 'companyIdRowsSql', args: ["id,category", "finance_accounts", " AND chart_id=? AND status='active' AND currency=? AND id IN (?,?)"],
    call: "companyIdRowsSql('id,category','finance_accounts',\" AND chart_id=? AND status='active' AND currency=? AND id IN (?,?)\")", expected: "SELECT id,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND currency=? AND id IN (?,?)",
    sha: '0b1d256d00d1b60fabd493399ca4a00f9757a5d79fffdf043881b824df045519', label: "fx adjustment accounts" },
];

const render = (cs: Case) => cs.helper === 'bankMatchGuardSql'
  ? bankMatchGuardSql(cs.args[0])
  : cs.helper === 'companyIdRowsSql'
    ? companyIdRowsSql(cs.args[0], cs.args[1], cs.args[2])
    : receiptIdByPaymentSql(cs.args[0]);

// Reconstructed independently of the helper bodies: the closed byte oracle.
const oracle = (cs: Case) => cs.helper === 'bankMatchGuardSql'
  ? `${BANK_HEAD}${cs.args[0]})`
  : cs.helper === 'companyIdRowsSql'
    ? `SELECT ${cs.args[0]} FROM ${cs.args[1]} WHERE company_id=?${cs.args[2]}`
    : `SELECT id FROM finance_customer_receipts WHERE payment_id=?${cs.args[0]}`;

const imports = (file: string) => file === 'finance.ts'
  ? "import { bankMatchGuardSql, companyIdRowsSql } from './finance-read-tail-r56-sql';"
  : file === 'customer-receipt-issue.ts'
    ? "import { receiptIdByPaymentSql } from './finance-read-tail-r56-sql';"
    : file === 'finance-withholding-remittance.ts' || file === 'refunds.ts'
      ? "import { bankMatchGuardSql } from './finance-read-tail-r56-sql';"
      : "import { companyIdRowsSql } from './finance-read-tail-r56-sql';";

describe('Finance read-tail r56 closed read-tail renderers', () => {
  test('all 11 sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(11);
    for (const cs of CASES) {
      const got = render(cs);
      expect(got).toBe(cs.expected);
      expect(got).toBe(oracle(cs));
      expect(sha(got)).toBe(cs.sha);
      expect(count(got, 'SELECT ')).toBe(1);
      expect((got.match(/\?/g) ?? []).length).toBe((cs.args.join('').match(/\?/g) ?? []).length + 1);
    }
    // The 11 sites span five byte-identical head families.
    expect(CASES.filter(c => c.helper === 'bankMatchGuardSql')).toHaveLength(3);
    expect(CASES.filter(c => c.helper === 'companyIdRowsSql')).toHaveLength(6);
    expect(CASES.filter(c => c.helper === 'receiptIdByPaymentSql')).toHaveLength(2);
    expect(new Set(CASES.map(c => c.expected.split('?')[0])).size).toBe(5);
    expect(bankMatchGuardSql('')).not.toBe(companyIdRowsSql('id', 't', ''));
    expect(receiptIdByPaymentSql('')).toBe('SELECT id FROM finance_customer_receipts WHERE payment_id=?');
  });

  test('callers delegate with unchanged tails, binds and lock clauses', () => {
    for (const cs of CASES) {
      const src = read(cs.file);
      expect(norm(src)).toContain(norm(cs.call));
      expect(norm(src)).toContain(norm(imports(cs.file)));
      expect(src).not.toContain(`'${cs.expected}'`);
      expect(src).not.toContain(`"${cs.expected}"`);
    }
    // Receipt: the FOR UPDATE OF p payment lock and both bind lists stay inline.
    const receipt = read('customer-receipt-issue.ts');
    expect(receipt).toContain('FOR UPDATE OF p');
    expect(receipt).toContain('.bind(paymentId, actor.companyId)');
    expect(receipt).toContain('.bind(paymentId)');
    // Company money keeps the dynamic placeholder list and bind spread.
    const money = read('company-money.ts');
    expect(money).toContain("` AND currency IN (${currencies.map(() => '?').join(',')})`");
    expect(money).toContain('.bind(companyId, ...currencies)');
    expect(money).toContain("'SELECT scale FROM company_currency_scales WHERE company_id=? AND currency=?'");
    // Bank guards keep their interleaved bind orders.
    expect(read('finance-withholding-remittance.ts')).toContain('[input.bankTransactionId, actor.companyId, input.bankVersion, amount, input.bankReference, String(row.bank_account)]');
    expect(read('refunds.ts')).toContain("[input.bankTransactionId, actor.companyId, input.bankVersion, outgoing ? -amount : amount, owner.currency, row.bank_account]");
    expect(read('finance.ts')).toContain('[bank.id, actor.companyId, input.bankVersion, payment.bank_account, payment.currency, signed, payment.payment_date]');
    // Ledger list tails keep their ORDER BY / LIMIT and bind lists.
    expect(read('finance-assets.ts')).toContain("' ORDER BY end_date DESC,id'");
    expect(read('finance-engines.ts')).toContain("' ORDER BY start_date DESC LIMIT 24'");
    expect(read('finance-assets.ts')).toContain('.bind(actor.companyId, chart, input.proceedsAccountId ?? null, input.gainAccountId, input.lossAccountId)');
    expect(read('finance.ts')).toContain('.bind(actor.companyId, input.chartId, calculated.chart.base_currency, input.gainAccountId, input.lossAccountId)');
  });

  test('delegation counts match the 3 bank + 6 company-row + 2 receipt anchors', () => {
    expect(count(read('finance-withholding-remittance.ts'), 'bankMatchGuardSql(')).toBe(1);
    expect(count(read('refunds.ts'), 'bankMatchGuardSql(')).toBe(1);
    expect(count(read('finance.ts'), 'bankMatchGuardSql(')).toBe(1);
    expect(count(read('company-money.ts'), 'companyIdRowsSql(')).toBe(2);
    expect(count(read('finance-assets.ts'), 'companyIdRowsSql(')).toBe(2);
    expect(count(read('finance-engines.ts'), 'companyIdRowsSql(')).toBe(1);
    expect(count(read('finance.ts'), 'companyIdRowsSql(')).toBe(1);
    expect(count(read('customer-receipt-issue.ts'), 'receiptIdByPaymentSql(')).toBe(2);
  });
});

describe('Finance read-tail r56 protected families and retirement', () => {
  test('r54/r55 helper bodies, native plans, locks and other owners stay untouched', () => {
    const r54 = read('finance-read-tail-r54-sql.ts');
    expect(count(r54, 'SELECT ')).toBe(2);
    expect(r54).toContain('`SELECT to_jsonb(${alias})::text image FROM ${table} ${alias} WHERE ${tail}`');
    expect(r54).toContain('`SELECT to_jsonb(${alias})::text source FROM ${table} ${alias} WHERE ${tail}`');
    const r55 = read('finance-read-tail-r55-sql.ts');
    expect(count(r55, 'SELECT ')).toBe(2);
    expect(r55).toContain('`SELECT count(*) FROM ${table} WHERE company_id=?${tail}`');
    expect(r55).toContain('`SELECT count(*) FROM ${table} ${alias} WHERE ${alias}.company_id=p.company_id${tail}`');
    const locked = read('finance-withholding-remittance.ts');
    expect(locked).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(locked).toContain('const lock =');
    expect(read('supplier-payment-money.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(read('payroll-other-remittance-facts.ts')).toContain('LOCK TABLE finance_charts IN SHARE MODE');
    expect(read('native-finance-read.ts')).toContain("FINANCE_RD_SUPPLIER_BATCH_LIST = 'finance_rd_supplier_batch_list'");
    const imported = new Set(CASES.map(c => c.file));
    expect([...imported].sort()).toEqual([
      'company-money.ts', 'customer-receipt-issue.ts', 'finance-assets.ts',
      'finance-engines.ts', 'finance-withholding-remittance.ts', 'finance.ts', 'refunds.ts',
    ]);
    for (const file of imported) expect(file.startsWith('native-')).toBe(false);
  });

  test('each r56 helper is defined exactly once', () => {
    const helper = read('finance-read-tail-r56-sql.ts');
    expect(count(helper, 'export const bankMatchGuardSql =')).toBe(1);
    expect(count(helper, 'export const companyIdRowsSql =')).toBe(1);
    expect(count(helper, 'export const receiptIdByPaymentSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(3);
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
    // 11 head literals collapse into three helpers (-11 literals, +3 declarations: 1726 -> 1718; r57 then 1718 -> 1710; r58 then 1710 -> 1708; r59 then 1708 -> 1707).
    expect(CASES).toHaveLength(11);
    expect(total).toBe(1707);
    expect(total).toBe(1726 - 19);
    expect(total).toBeLessThan(1726);
  });
});
