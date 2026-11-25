import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  assetAccountJoinByIdSql,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r59-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Independent head map: the closed renderer must render head + raw tail exactly.
const HEAD = 'SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=?';

type Case = { tail: string; call: string; expected: string; sha: string; label: string };

const CASES: Case[] = [
  { tail: " AND a.company_id=?",
    call: ".prepare(assetAccountJoinByIdSql(\" AND a.company_id=?\")) .bind(id, companyId) .first<Row>()",
    expected: "SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=? AND a.company_id=?",
    sha: '90fe7efef9bf197cce2efcb133f636c3e7e7eb292b5a489a162a4f88793ba3a6', label: 'asset by id and company' },
  { tail: " AND a.company_id=? AND a.status='in_service'",
    call: ".prepare(assetAccountJoinByIdSql(\" AND a.company_id=? AND a.status='in_service'\")) .bind(assetId, companyId) .first<Row>()",
    expected: "SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=? AND a.company_id=? AND a.status='in_service'",
    sha: '51352ba8b8f2350518388f0c4b7051b078a2ee38e1fedf24b100b2df942cb684', label: 'in-service disposal asset' },
];

// Reconstructed independently of the helper body: the closed byte oracle.
const oracle = (cs: Case) => HEAD + cs.tail;

describe('Finance read-tail r59 asset read renderer', () => {
  test('both sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(2);
    for (const cs of CASES) {
      const got = assetAccountJoinByIdSql(cs.tail);
      expect(got).toBe(cs.expected);
      expect(got).toBe(oracle(cs));
      expect(sha(got)).toBe(cs.sha);
      expect(got.startsWith(HEAD)).toBe(true);
      expect(got.slice(HEAD.length)).toBe(cs.tail);
    }
    // One byte-identical head family with two distinct rendered tails.
    expect(new Set(CASES.map(c => c.expected.split('?')[0])).size).toBe(1);
    expect(new Set(CASES.map(c => c.sha)).size).toBe(2);
    expect(assetAccountJoinByIdSql('')).toBe(HEAD);
    expect(sha(HEAD)).toBe('f302fe018a0150322657959a4f04ebe8d4fa9d5c2eb54bdaf11e1226097e8fcb');
  });

  test('callers delegate with unchanged tails, binds and result types', () => {
    const src = read('finance-asset-policies.ts');
    for (const cs of CASES) {
      expect(norm(src)).toContain(norm(cs.call));
      expect(src).toContain('"' + cs.tail + '"');
      for (const q of ['"', "'", '`']) expect(src).not.toContain(q + cs.expected + q);
    }
    // Binds are unchanged: both point reads keep id/company order and first<Row>().
    expect(norm(src)).toContain('.bind(id, companyId) .first<Row>()');
    expect(norm(src)).toContain('.bind(assetId, companyId) .first<Row>()');
  });

  test('delegation count matches the 2-site anchor', () => {
    expect(count(read('finance-asset-policies.ts'), 'assetAccountJoinByIdSql(')).toBe(2);
  });
});

describe('Finance read-tail r59 protected families and retirement', () => {
  test('r54-r58 helpers, native plans, locks, tax and r38 constants stay untouched', () => {
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
    expect(read('supplier-payment-money.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(read('payroll-other-remittance-facts.ts')).toContain('LOCK TABLE finance_charts IN SHARE MODE');
    expect(read('native-finance-read.ts')).toContain("FINANCE_RD_SUPPLIER_BATCH_LIST = 'finance_rd_supplier_batch_list'");
    expect(read('finance-tax-register.ts')).toContain('finance_vat_corrections');
    expect(read('finance-read-tail-r38-sql.ts')).toContain(
      'POS_CUSTOMER_PARTY_ACTIVE_SQL = "SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind=\'customer\' AND status=\'active\'"');
    expect(read('finance-read-tail-r38-sql.ts')).toContain(
      "CALCULATION_RUN_POINT_SQL = 'SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?'");
    // The selected family is not any protected body.
    expect('finance-asset-policies.ts'.startsWith('native-')).toBe(false);
    expect('finance-asset-policies.ts'.includes('tax')).toBe(false);
    expect('finance-asset-policies.ts'.includes('read-tail-r5')).toBe(false);
  });

  test('the r59 helper is defined once and renders the closed head', () => {
    const helper = read('finance-read-tail-r59-sql.ts');
    expect(count(helper, 'export const assetAccountJoinByIdSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=?${tail}`');
    expect(helper).not.toContain('FOR UPDATE');
    expect(helper).not.toContain('FOR SHARE');
    expect(helper).not.toContain('LOCK TABLE');
  });

  test('retirement:ts-statements pins the r63 target of 1702', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 2 head literals collapse into one renderer (-2 literals, +1 declaration: 1708 -> 1707); r60 then 1707 -> 1705; r61 then 1705 -> 1704; r62 then 1704 -> 1703; r63 then 1703 -> 1702).
    expect(CASES).toHaveLength(2);
    expect(total).toBe(1702);
    expect(total).toBe(1708 - 6);
    expect(total).toBeLessThan(1708);
  });
});
