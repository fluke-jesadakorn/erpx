import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  FINANCE_SCOPE_BIND_SQL,
  FINANCE_SCOPE_NULL_BIND_SQL,
  jsonbArrayTextSql,
  UNMATCHED_BANK_TXN_COUNT_SQL,
  AI_USAGE_MONTH_SQL,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r44-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const helper = read('finance-read-tail-r44-sql.ts');
const count = (src: string, needle: string) => src.split(needle).length - 1;

const SCOPE = 'SELECT ?::text company_id,?::text branch_id,?::text department_id';
const SCOPE_NULL = 'SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id';
const JSONB_HEAD = 'SELECT jsonb_array_elements_text(?::jsonb)';
const BANK_UNMATCHED = "SELECT count(*) FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched'";
const AI_USAGE = 'SELECT company_id,month FROM finance_ai_usage';

describe('Finance read-tail r44 byte-identical NEW families', () => {
  test('workforce scope bind renders byte-identically at all five non-assertion sites', () => {
    expect(FINANCE_SCOPE_BIND_SQL).toBe(SCOPE);
    const doc = read('document-purchases.ts');
    const src = [doc, read('legal-setup.ts'), read('source-purchases.ts'), read('sales-customer-receipts.ts')];
    expect(count(doc, '${FINANCE_SCOPE_BIND_SQL}')).toBe(2);
    expect(count(src[1], '${FINANCE_SCOPE_BIND_SQL}')).toBe(1);
    expect(count(src[2], '${FINANCE_SCOPE_BIND_SQL}')).toBe(1);
    expect(count(src[3], '${FINANCE_SCOPE_BIND_SQL}')).toBe(1);
    for (const s of src) expect(s).not.toContain(SCOPE);
    // Assertion guard and r35 helper stay inline (skipped).
    expect(read('attachment-purchases.ts')).toContain(SCOPE);
    expect(read('finance-read-tail-r35-sql.ts')).toContain(SCOPE);
  });

  test('NULL-scope bind renders byte-identically at all four sites', () => {
    expect(FINANCE_SCOPE_NULL_BIND_SQL).toBe(SCOPE_NULL);
    const files = ['sales-posting-setup.ts', 'expense-posting-prerequisites.ts', 'assets-financial-entry.ts', 'payroll-posting-prerequisites.ts'];
    for (const name of files) {
      expect(count(read(name), '${FINANCE_SCOPE_NULL_BIND_SQL}')).toBe(1);
      expect(read(name)).not.toContain(SCOPE_NULL);
    }
  });

  test('jsonb text array renders byte-identically at all seven sites including the aliased tail', () => {
    expect(jsonbArrayTextSql('')).toBe(JSONB_HEAD);
    expect(jsonbArrayTextSql(' id')).toBe(`${JSONB_HEAD} id`);
    expect(count(read('inventory-attachment-sources.ts'), "jsonbArrayTextSql('')")).toBe(1);
    expect(count(read('calendar-refunds-facts.ts'), "jsonbArrayTextSql('')")).toBe(1);
    expect(count(read('commerce-attachment-sources.ts'), "jsonbArrayTextSql('')")).toBe(2);
    expect(count(read('assets-financial-sources.ts'), "jsonbArrayTextSql(' id')")).toBe(1);
    expect(count(read('manufacturing-existing-posting-setup.ts'), "jsonbArrayTextSql(' id')")).toBe(2);
    // The nested string-argument site uses concatenation so it renders the same bytes.
    expect(read('commerce-attachment-sources.ts')).toContain("' AND p.id IN(' + jsonbArrayTextSql('') + ')'");
    for (const name of ['inventory-attachment-sources.ts', 'calendar-refunds-facts.ts', 'assets-financial-sources.ts', 'manufacturing-existing-posting-setup.ts']) {
      expect(read(name)).not.toContain(JSONB_HEAD);
    }
  });

  test('unmatched bank-transaction count renders byte-identically at both sites', () => {
    expect(UNMATCHED_BANK_TXN_COUNT_SQL).toBe(BANK_UNMATCHED);
    for (const name of ['finance-workspace.ts', 'finance-invoice-read.ts']) {
      expect(count(read(name), '${UNMATCHED_BANK_TXN_COUNT_SQL}')).toBe(1);
      expect(read(name)).not.toContain(BANK_UNMATCHED);
    }
  });

  test('AI usage month projection renders byte-identically at all three non-assertion sites', () => {
    expect(AI_USAGE_MONTH_SQL).toBe(AI_USAGE);
    expect(count(read('finance-ai.ts'), '${AI_USAGE_MONTH_SQL}')).toBe(2);
    expect(read('records-ai-usage-projection.ts')).toContain('export const recordsFinanceUsageSql = AI_USAGE_MONTH_SQL;');
    expect(read('finance-ai.ts')).toContain(AI_USAGE); // assertion-guard site stays inline (skipped)
    expect(read('finance-tax-register.ts')).toContain('SELECT'); // tax is skipped
    expect(read('finance-read-tail-r43-sql.ts')).toContain('journalLineCountSql'); // r15-r43 stay in place
  });

  test('each r44 family is defined exactly once', () => {
    for (const name of ['FINANCE_SCOPE_BIND_SQL', 'FINANCE_SCOPE_NULL_BIND_SQL', 'jsonbArrayTextSql', 'UNMATCHED_BANK_TXN_COUNT_SQL', 'AI_USAGE_MONTH_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(read('finance-read-tail-r35-sql.ts')).toContain('WORKFORCE_ACTOR_SCOPE_SQL');
    expect(read('finance-read-tail-r43-sql.ts')).toContain('BOOKS_AGGREGATE_IMAGE_SQL');
  });
});

describe('Finance read-tail r44 retirement check', () => {
  test('retirement:ts-statements stays strictly below BEFORE=1884 and at or below 1805', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r44 collapsed 21 inline into five helpers (1884 -> 1868); r45 removes eight more (-> 1860); r46 removes nine more (-> 1851); r48 removes seven more (-> 1844); r49 removes 32 more (-> 1812); r50 removes seven more (-> 1805); r51 removes 22 (-> 1783); r52 removes 19 (-> 1764); r53 removes 10 (-> 1754); r54 routes 14 point reads into two helpers (-> 1742).
    // r55 folds 18 company count literals into two helpers (-18 literals, +2 declarations: 1742 -> 1726).
    // r56 folds 11 head literals into three helpers (-11 literals, +3 declarations: 1726 -> 1718).
    expect(total).toBe(1703);
    expect(total).toBeLessThanOrEqual(1718);
    expect(total).toBeLessThan(1884);
  });
});
