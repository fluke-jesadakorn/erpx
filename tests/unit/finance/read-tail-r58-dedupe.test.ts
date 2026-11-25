import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  finCustomerPartyIdSql, finCustomerPartyListSql,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r58-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

type Helper = 'finCustomerPartyIdSql' | 'finCustomerPartyListSql';
type Case = { file: string; helper: Helper; tail: string; call: string; expected: string; sha: string; label: string };

// Independent head map: the closed renderers must render head + raw tail exactly.
const HEADS: Record<Helper, string> = {
  finCustomerPartyIdSql: 'SELECT id FROM finance_parties WHERE id=?',
  finCustomerPartyListSql: 'SELECT id,name,currency FROM finance_parties WHERE company_id=?',
};

const CASES: Case[] = [
  { file: 'finance.ts', helper: 'finCustomerPartyIdSql', tail: " AND company_id=? AND kind='customer'",
    call: 'write.prepare(finCustomerPartyIdSql(" AND company_id=? AND kind=\'customer\'")).bind(input.customerId, actor.companyId)',
    expected: "SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer'",
    sha: '4db719a88f93d1ff6569f44eed5b2bce8cb97b721ad72e3c10de338884d4c7d7', label: 'finance revenue-customer point read' },
  { file: 'field-service-references.ts', helper: 'finCustomerPartyListSql', tail: " AND kind='customer' AND status='active' ORDER BY name,id",
    call: 'database.prepare(finCustomerPartyListSql(" AND kind=\'customer\' AND status=\'active\' ORDER BY name,id")) .bind(companyId).all<ServiceCustomerReference>()',
    expected: "SELECT id,name,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name,id",
    sha: '53139743efd15346b1ff782fa127aba50627f1813afd340bd4e18e2e69887d92', label: 'field-service customer reference list' },
  { file: 'finance-business-contracts.ts', helper: 'finCustomerPartyListSql', tail: " AND kind='customer' AND status='active' AND person_id IS NULL ORDER BY name",
    call: '.prepare(finCustomerPartyListSql(" AND kind=\'customer\' AND status=\'active\' AND person_id IS NULL ORDER BY name")) .bind(companyId) .all<Row>()',
    expected: "SELECT id,name,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND person_id IS NULL ORDER BY name",
    sha: '49c05a75648408272a2df6fca5c9ea999b72bd319e0b638dec9a34e44844f6bb', label: 'business-contract workspace customer list' },
];

const render = (cs: Case) => ({
  finCustomerPartyIdSql: () => finCustomerPartyIdSql(cs.tail),
  finCustomerPartyListSql: () => finCustomerPartyListSql(cs.tail),
})[cs.helper]();

// Reconstructed independently of the helper bodies: the closed byte oracle.
const oracle = (cs: Case) => HEADS[cs.helper] + cs.tail;

describe('Finance read-tail r58 customer read renderers', () => {
  test('all 3 remaining sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(3);
    for (const cs of CASES) {
      const got = render(cs);
      expect(got).toBe(cs.expected);
      expect(got).toBe(oracle(cs));
      expect(sha(got)).toBe(cs.sha);
      expect(got.startsWith(HEADS[cs.helper])).toBe(true);
      expect(got.slice(HEADS[cs.helper].length)).toBe(cs.tail);
    }
    // The three remaining sites span two byte-identical head families.
    expect(CASES.filter(c => c.helper === 'finCustomerPartyIdSql')).toHaveLength(1);
    expect(CASES.filter(c => c.helper === 'finCustomerPartyListSql')).toHaveLength(2);
    expect(new Set(CASES.map(c => c.expected.split('?')[0])).size).toBe(2);
    expect(new Set(CASES.map(c => c.sha)).size).toBe(3);
    expect(finCustomerPartyIdSql('')).toBe(HEADS.finCustomerPartyIdSql);
    expect(finCustomerPartyListSql('')).toBe(HEADS.finCustomerPartyListSql);
  });

  test('callers delegate with unchanged tails, binds and result types', () => {
    for (const cs of CASES) {
      const src = read(cs.file);
      expect(norm(src)).toContain(norm(cs.call));
      expect(src).toContain(cs.tail);
      for (const q of ['"', "'", '`']) expect(src).not.toContain(q + cs.expected + q);
    }
    // Binds are unchanged: point reads keep id/company/currency order; lists keep company only.
    expect(norm(read('finance.ts'))).toContain('.bind(input.customerId, actor.companyId)');
    // sales-customer-sources delegated to the native lane 5 reader: no SQL, no bind.
    const customers = read('sales-customer-sources.ts');
    expect(customers).toContain("from './native-lane-5/client'");
    expect(customers).not.toMatch(/\bSELECT\b|\.prepare\(|\.bind\(/);
    expect(norm(read('field-service-references.ts'))).toContain('.bind(companyId).all<ServiceCustomerReference>()');
    expect(norm(read('finance-business-contracts.ts'))).toContain('.bind(companyId) .all<Row>()');
  });

  test('delegation counts match the 1+2 anchors', () => {
    const calls = (file: string, helper: Helper) => count(read(file), helper + '(');
    expect(calls('finance.ts', 'finCustomerPartyIdSql')).toBe(1);
    expect(calls('sales-customer-sources.ts', 'finCustomerPartyIdSql')).toBe(0);
    expect(calls('field-service-references.ts', 'finCustomerPartyListSql')).toBe(1);
    expect(calls('finance-business-contracts.ts', 'finCustomerPartyListSql')).toBe(1);
  });
});

describe('Finance read-tail r58 protected families and retirement', () => {
  test('r54-r57 helpers, native plans, locks, tax and r38 constants stay untouched', () => {
    const r54 = read('finance-read-tail-r54-sql.ts');
    expect(count(r54, 'SELECT ')).toBe(2);
    expect(r54).toContain('`SELECT to_jsonb(${alias})::text image FROM ${table} ${alias} WHERE ${tail}`');
    const r55 = read('finance-read-tail-r55-sql.ts');
    expect(count(r55, 'SELECT ')).toBe(2);
    expect(r55).toContain('`SELECT count(*) FROM ${table} WHERE company_id=?${tail}`');
    const r56 = read('finance-read-tail-r56-sql.ts');
    expect(count(r56, 'SELECT ')).toBe(3);
    expect(count(read('finance-read-tail-r57-sql.ts'), 'SELECT ')).toBe(4);
    expect(read('supplier-payment-money.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(read('payroll-other-remittance-facts.ts')).toContain('LOCK TABLE finance_charts IN SHARE MODE');
    expect(read('native-finance-read.ts')).toContain("FINANCE_RD_SUPPLIER_BATCH_LIST = 'finance_rd_supplier_batch_list'");
    expect(read('finance-tax-register.ts')).toContain('finance_vat_corrections');
    expect(read('finance-read-tail-r38-sql.ts')).toContain(
      'POS_CUSTOMER_PARTY_ACTIVE_SQL = "SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind=\'customer\' AND status=\'active\'"');
    expect(read('finance-read-tail-r38-sql.ts')).toContain(
      "CALCULATION_RUN_POINT_SQL = 'SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?'");
    const imported = [...new Set(CASES.map(c => c.file))].sort();
    expect(imported).toEqual(['field-service-references.ts', 'finance-business-contracts.ts',
      'finance.ts']);
    for (const file of imported) {
      expect(file.startsWith('native-')).toBe(false);
      expect(file.includes('tax')).toBe(false);
      expect(file.includes('read-tail-r5')).toBe(false);
    }
  });

  test('each r58 helper is defined exactly once', () => {
    const helper = read('finance-read-tail-r58-sql.ts');
    for (const name of ['finCustomerPartyIdSql', 'finCustomerPartyListSql']) {
      expect(count(helper, 'export const ' + name + ' =')).toBe(1);
    }
    expect(count(helper, 'SELECT ')).toBe(2);
    expect(helper).toContain('`SELECT id FROM finance_parties WHERE id=?${tail}`');
    expect(helper).toContain('`SELECT id,name,currency FROM finance_parties WHERE company_id=?${tail}`');
  });

  test('retirement:ts-statements pins the current owner target of 1693', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 4 head literals collapse into two renderers (-4 literals, +2 declarations: 1710 -> 1708); r59 then 1708 -> 1707; r60 then 1707 -> 1705; r61 then 1705 -> 1704; r62 then 1704 -> 1703; r63 then 1703 -> 1702; r64 then 1702 -> 1701; native cutovers then 1701 -> 1693).
    expect(CASES).toHaveLength(3);
    expect(total).toBe(1693);
    expect(total).toBe(1710 - 17);
    expect(total).toBeLessThan(1710);
  });
});
