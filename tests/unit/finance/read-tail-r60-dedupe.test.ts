import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  groupEntitiesByHoldingSql,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r60-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const norm = (src: string) => src.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Independent head map: the closed renderer must render head + raw tail exactly.
const HEAD = 'SELECT * FROM finance_group_entities WHERE holding_id=?';
const HEAD_SHA = 'efd5dfa025403a06b011bcf8aec2e4360c041e1386d93913e2d1282e5d92a71b';

type Case = {
  file: string; tail: string; call: string; binds: string; bindCount: number;
  expected: string; sha: string; label: string;
};

const CASES: Case[] = [
  { file: 'finance-group-ownership.ts',
    tail: " AND company_id IN (?,?) AND status='active' ORDER BY company_id",
    call: 'groupEntitiesByHoldingSql(" AND company_id IN (?,?) AND status=\'active\' ORDER BY company_id")',
    binds: '.bind(holdingId, investorCompanyId, investeeCompanyId) .all<Row>()',
    bindCount: 3,
    expected: "SELECT * FROM finance_group_entities WHERE holding_id=? AND company_id IN (?,?) AND status='active' ORDER BY company_id",
    sha: '3fc6e09754cee2eaac2241f5d68b56d4e3ccadb862e1fc68c1b91cfa37cbf0f6',
    label: 'ownership source entity pair read' },
  { file: 'finance-group-ownership.ts',
    tail: " AND status='active' ORDER BY reporting_name,id",
    call: 'groupEntitiesByHoldingSql(" AND status=\'active\' ORDER BY reporting_name,id")',
    binds: '.bind(holdingId) .all<Row>()',
    bindCount: 1,
    expected: "SELECT * FROM finance_group_entities WHERE holding_id=? AND status='active' ORDER BY reporting_name,id",
    sha: '0a57ad417e169c01b00b05a88e1bceac16a4c2b13eef29aa0f500019bf83dc3f',
    label: 'group workspace entity list read' },
  { file: 'finance-intercompany-pairs.ts',
    tail: " AND company_id=? AND status='active'",
    call: 'groupEntitiesByHoldingSql(" AND company_id=? AND status=\'active\'")',
    binds: '.bind(holdingId, companyId) .first<Row>()',
    bindCount: 2,
    expected: "SELECT * FROM finance_group_entities WHERE holding_id=? AND company_id=? AND status='active'",
    sha: 'fa93f46ba921239dc9b049749160f0dc0b746b4e903004aa1afdf48d3838c747',
    label: 'intercompany pair entity point read' },
];

// Reconstructed independently of the helper body: the closed byte oracle.
const oracle = (cs: Case) => HEAD + cs.tail;

describe('Finance read-tail r60 group-entity read renderer', () => {
  test('all 3 sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(3);
    for (const cs of CASES) {
      const got = groupEntitiesByHoldingSql(cs.tail);
      expect(got, cs.label).toBe(cs.expected);
      expect(got, cs.label).toBe(oracle(cs));
      expect(sha(got), cs.label).toBe(cs.sha);
      expect(got.startsWith(HEAD), cs.label).toBe(true);
      expect(got.slice(HEAD.length), cs.label).toBe(cs.tail);
      expect((got.match(/\?/g) ?? []).length, cs.label).toBe(cs.bindCount);
    }
    // One byte-identical bind-bearing head with three distinct rendered tails.
    expect(new Set(CASES.map(c => c.tail)).size).toBe(3);
    expect(new Set(CASES.map(c => c.sha)).size).toBe(3);
    expect(groupEntitiesByHoldingSql('')).toBe(HEAD);
    expect(sha(HEAD)).toBe(HEAD_SHA);
  });

  test('callers delegate with unchanged tails, binds and result types', () => {
    for (const cs of CASES) {
      const src = read(cs.file);
      expect(norm(src), cs.label).toContain(norm(cs.call));
      expect(norm(src), cs.label).toContain(norm(cs.binds));
      expect(src, cs.label).toContain(cs.tail);
      for (const q of ['"', "'", '`']) expect(src, cs.label).not.toContain(q + cs.expected + q);
    }
  });

  test('delegation counts match the 2+1 anchor', () => {
    expect(count(read('finance-group-ownership.ts'), 'groupEntitiesByHoldingSql(')).toBe(2);
    expect(count(read('finance-intercompany-pairs.ts'), 'groupEntitiesByHoldingSql(')).toBe(1);
  });
});

describe('Finance read-tail r60 protected families and retirement', () => {
  test('r54-r59 helpers, native plans, locks, tax and r38 constants stay untouched', () => {
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
      expect(cs.file.includes('read-tail-r5')).toBe(false);
    }
  });

  test('the r60 helper is defined once and renders the closed head', () => {
    const helper = read('finance-read-tail-r60-sql.ts');
    expect(count(helper, 'export const groupEntitiesByHoldingSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT * FROM finance_group_entities WHERE holding_id=?${tail}`');
    expect(helper).not.toContain('FOR UPDATE');
    expect(helper).not.toContain('FOR SHARE');
    expect(helper).not.toContain('LOCK TABLE');
    expect(helper).not.toMatch(/pg_advisory/i);
  });

  test('retirement:ts-statements pins the current owner target of 1607', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // 3 head literals collapse into one renderer (-3 literals, +1 declaration: 1707 -> 1705); r61 then 1705 -> 1704; r62 then 1704 -> 1703.
    expect(CASES).toHaveLength(3);
    expect(total).toBe(1607);
    expect(total).toBe(1607);
    expect(total).toBeLessThan(1707);
  });
});
