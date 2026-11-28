import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  instrumentRunExistsSql,
} from '../../../modules/finance/server/src/adapters/finance-read-tail-r64-sql';

const srcRoot = resolve(__dirname, '../../../modules/finance/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Independent head map: the closed renderer must render head + raw tail exactly.
const HEAD = 'SELECT 1 FROM finance_instrument_runs WHERE instrument_id=?';
const HEAD_SHA = '334122888583580dbe3502954f16f5a2eaf75aad14c58889931e412908b145a1';

type Case = {
  file: string; tail: string; call: string; binds: string; bindCount: number;
  expected: string; sha: string; label: string;
};

const CASES: Case[] = [
  { file: 'finance-instruments.ts',
    tail: " AND status='draft'",
    call: 'instrumentRunExistsSql(" AND status=\'draft\'")',
    binds: '.bind(id) .first()',
    bindCount: 1,
    expected: "SELECT 1 FROM finance_instrument_runs WHERE instrument_id=? AND status='draft'",
    sha: '18a8eebfd55d9ead41ed10d9ebb27d5f82902c86abca1ec1d8054677f532a023',
    label: 'pending instrument-run existence' },
  { file: 'finance-instruments.ts',
    tail: " AND ((status='posted' AND ordinal>?) OR status='draft')",
    call: 'instrumentRunExistsSql(" AND ((status=\'posted\' AND ordinal>?) OR status=\'draft\')")',
    binds: '.bind(id, ordinal) .first()',
    bindCount: 2,
    expected: "SELECT 1 FROM finance_instrument_runs WHERE instrument_id=? AND ((status='posted' AND ordinal>?) OR status='draft')",
    sha: '9911b18fb490ca1080a93b2e3a29f3bbbd711cc1a93b96684d703dd8ad54c77b',
    label: 'later instrument-run existence' },
];

// Reconstructed independently of the helper body: the closed byte oracle.
const oracle = (cs: Case) => HEAD + cs.tail;

describe('Finance read-tail r64 instrument-run existence renderer', () => {
  test('both sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(2);
    for (const cs of CASES) {
      const got = instrumentRunExistsSql(cs.tail);
      expect(got, cs.label).toBe(cs.expected);
      expect(got, cs.label).toBe(oracle(cs));
      expect(sha(got), cs.label).toBe(cs.sha);
      expect(got.startsWith(HEAD), cs.label).toBe(true);
      expect(got.slice(HEAD.length), cs.label).toBe(cs.tail);
      expect((got.match(/\?/g) ?? []).length, cs.label).toBe(cs.bindCount);
    }
    // One byte-identical bind-bearing head with two distinct rendered tails.
    expect(new Set(CASES.map(c => c.tail)).size).toBe(2);
    expect(new Set(CASES.map(c => c.sha)).size).toBe(2);
    expect(instrumentRunExistsSql('')).toBe(HEAD);
    expect(sha(HEAD)).toBe(HEAD_SHA);
  });

  test('callers delegate with unchanged tails, binds and result types', () => {
    for (const cs of CASES) {
      const src = read(cs.file);
      expect(norm(src), cs.label).toContain(norm(cs.call));
      expect(norm(src), cs.label).toContain(norm(cs.binds));
      expect(src, cs.label).toContain('"' + cs.tail + '"');
      for (const q of ['"', "'", '`']) expect(src, cs.label).not.toContain(q + cs.expected + q);
    }
  });

  test('delegation counts match the 1+1 anchor', () => {
    expect(count(read('finance-instruments.ts'), 'instrumentRunExistsSql(')).toBe(2);
  });
});

describe('Finance read-tail r64 protected families and retirement', () => {
  test('r54-r63 helpers, native plans, locks, tax and r38 constants stay untouched', () => {
    const r54 = read('finance-read-tail-r54-sql.ts');
    expect(count(r54, 'SELECT ')).toBe(2);
    expect(r54).toContain('`SELECT to_jsonb(${alias})::text image FROM ${table} ${alias} WHERE ${tail}`');
    const r55 = read('finance-read-tail-r55-sql.ts');
    expect(count(r55, 'SELECT ')).toBe(2);
    expect(r55).toContain('`SELECT count(*) FROM ${table} WHERE company_id=?${tail}`');
    const r56 = read('finance-read-tail-r56-sql.ts');
    expect(count(r56, 'SELECT ')).toBe(3);
    expect(count(read('finance-read-tail-r57-sql.ts'), 'SELECT ')).toBe(4);
    expect(count(read('finance-read-tail-r58-sql.ts'), 'SELECT ')).toBe(2);
    const r59 = read('finance-read-tail-r59-sql.ts');
    expect(count(r59, 'SELECT ')).toBe(1);
    expect(r59).toContain('`SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=?${tail}`');
    const r60 = read('finance-read-tail-r60-sql.ts');
    expect(count(r60, 'SELECT ')).toBe(1);
    expect(r60).toContain('`SELECT * FROM finance_group_entities WHERE holding_id=?${tail}`');
    const r61 = read('finance-read-tail-r61-sql.ts');
    expect(count(r61, 'SELECT ')).toBe(1);
    expect(r61).toContain('`SELECT primary_chart_id FROM finance_book_rules WHERE ${tail}`');
    const r62 = read('finance-read-tail-r62-sql.ts');
    expect(count(r62, 'SELECT ')).toBe(1);
    expect(r62).toContain('`SELECT * FROM finance_group_report_editions r WHERE ${tail}`');
    const r63 = read('finance-read-tail-r63-sql.ts');
    expect(count(r63, 'SELECT ')).toBe(1);
    expect(r63).toContain('`SELECT 1 FROM finance_recognition_runs WHERE ${tail}`');
    expect(read('supplier-payment-money.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(read('payroll-other-remittance-facts.ts')).toContain('LOCK TABLE finance_charts IN SHARE MODE');
    expect(read('native-finance-read.ts')).toContain("FINANCE_RD_SUPPLIER_BATCH_LIST = 'finance_rd_supplier_batch_list'");
    expect(read('finance-tax-register.ts')).toContain('finance_vat_corrections');
    expect(read('finance-read-tail-r38-sql.ts')).toContain(
      'POS_CUSTOMER_PARTY_ACTIVE_SQL = "SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind=\'customer\' AND status=\'active\'"');
    expect(read('finance-read-tail-r38-sql.ts')).toContain(
      "CALCULATION_RUN_POINT_SQL = 'SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?'");
    for (const cs of CASES) {
      expect(cs.file.startsWith('native-')).toBe(false);
      expect(cs.file.includes('tax')).toBe(false);
      expect(cs.file.includes('read-tail-r')).toBe(false);
    }
  });

  test('the r64 helper is defined once and renders the closed head', () => {
    const helper = read('finance-read-tail-r64-sql.ts');
    expect(count(helper, 'export const instrumentRunExistsSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT 1 FROM finance_instrument_runs WHERE instrument_id=?${tail}`');
    expect(helper).not.toContain('FOR UPDATE');
    expect(helper).not.toContain('FOR SHARE');
    expect(helper).not.toContain('LOCK TABLE');
    expect(helper).not.toMatch(/pg_advisory/i);
  });

  test('retirement:ts-statements pins the current owner target of 1562', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 2 head literals collapse into one renderer (-2 literals, +1 declaration: 1702 -> 1701; native cutovers then 1701 -> 1680).
    expect(CASES).toHaveLength(2);
    expect(total).toBe(1562);
    expect(total).toBe(1562);
    expect(total).toBe(1562);
    expect(total).toBeLessThan(1704);
  });
});
