import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  planEditionPointSql,
  bankTxnGuardSql,
  acquisitionReadSql,
  acquisitionVersionGuardSql,
} from '../../../modules/finance/server/src/adapters/finance-read-tail-r48-sql';

const srcRoot = resolve(__dirname, '../../../modules/finance/server/src');
const read = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;

const PLAN_BASE = 'SELECT * FROM finance_plan_editions WHERE id=? AND company_id=?';
const PARENT = "SELECT * FROM finance_plan_editions WHERE id=? AND company_id=? AND chart_id=? AND kind=? AND status='approved'";
const PLAN = "SELECT * FROM finance_plan_editions WHERE id=? AND company_id=? AND kind=? AND status='approved'";
const TXN_BASE = 'EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=?';
const PROPOSED = "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND version=? AND match_status IN ('unmatched','proposed'))";
const GROUP_MATCH = "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND match_status='matched' AND matched_type='group' AND matched_id=?)";
const JOURNAL_MATCH = "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND match_status='matched' AND matched_type='journal' AND matched_id=?)";
const ACQ_APPROVED = "SELECT * FROM finance_acquisitions WHERE id=? AND holding_id=? AND company_id=? AND status='approved' AND currency=? AND acquisition_date<=?";
const ACQ_EFFECT = "SELECT * FROM finance_acquisitions WHERE id=? AND holding_id=? AND investee_entity_id=? AND status IN ('approved','impaired') AND acquisition_date<=? AND currency=? AND calculation_run_id=?";
const ACQ_SOURCE = "SELECT 1 FROM finance_acquisitions WHERE company_id=? AND ownership_id=? AND acquisition_date=? AND currency=? AND source_hash IS NOT NULL AND status IN ('draft','approved','impaired') AND id NOT IN (?,?)";
const ACQ_PARENT = "EXISTS(SELECT 1 FROM finance_acquisitions WHERE id=? AND company_id=? AND holding_id=? AND status='approved' AND version=?)";
const ACQ_VERSION = "EXISTS(SELECT 1 FROM finance_acquisitions WHERE id=? AND company_id=? AND holding_id=? AND status='draft' AND version=?)";

describe('Finance read-tail r48 Rank1 plan edition point', () => {
  test('renders byte-identically to both retired inline literals', () => {
    expect(planEditionPointSql(" AND chart_id=? AND kind=? AND status='approved'")).toBe(PARENT);
    expect(planEditionPointSql(" AND kind=? AND status='approved'")).toBe(PLAN);
    expect(planEditionPointSql('')).toBe(PLAN_BASE);
    expect(planEditionPointSql(" AND kind=? AND status='approved'")).not.toBe(PARENT);
  });

  test('both planning sites call the helper while the r38 point constant stays in use', () => {
    const planning = read('finance-planning.ts');
    expect(count(planning, 'planEditionPointSql(')).toBe(2);
    expect(planning).not.toContain(PARENT);
    expect(planning).not.toContain(PLAN);
    expect(planning).toContain('planEditionPointSql(" AND chart_id=? AND kind=? AND status=\'approved\'")');
    expect(planning).toContain('planEditionPointSql(" AND kind=? AND status=\'approved\'")');
    // decisionRow and recordRow keep the r38 constant and their binds.
    expect(count(planning, 'PLAN_EDITION_POINT_SQL')).toBe(3); // import + two point reads
    expect(read('finance-read-tail-r38-sql.ts')).toContain(`export const PLAN_EDITION_POINT_SQL = '${PLAN_BASE}';`);
  });
});

describe('Finance read-tail r48 Rank2 bank transaction guards', () => {
  test('renders all three matched/unmatched clauses byte-identically', () => {
    expect(bankTxnGuardSql(" AND version=? AND match_status IN ('unmatched','proposed')")).toBe(PROPOSED);
    expect(bankTxnGuardSql(" AND match_status='matched' AND matched_type='group' AND matched_id=?")).toBe(GROUP_MATCH);
    expect(bankTxnGuardSql(" AND match_status='matched' AND matched_type='journal' AND matched_id=?")).toBe(JOURNAL_MATCH);
    expect(bankTxnGuardSql('')).toBe(TXN_BASE + ')');
    expect(bankTxnGuardSql(" AND match_status='matched' AND matched_type='group' AND matched_id=?"))
      .not.toBe(bankTxnGuardSql(" AND match_status='matched' AND matched_type='journal' AND matched_id=?"));
  });

  test('all three guards delegate while paired native calls stay untouched', () => {
    const bank = read('finance-bank-automation.ts');
    const instruments = read('finance-instruments.ts');
    expect(count(bank, 'bankTxnGuardSql(')).toBe(2);
    expect(count(instruments, 'bankTxnGuardSql(')).toBe(1);
    for (const src of [bank, instruments]) {
      expect(src).not.toContain(TXN_BASE);
      expect(src).not.toContain(GROUP_MATCH);
      expect(src).not.toContain(JOURNAL_MATCH);
    }
    expect(bank).toContain('bankTxnGuardSql(" AND version=? AND match_status IN (\'unmatched\',\'proposed\')")');
    expect(bank).toContain('bankTxnGuardSql(" AND match_status=\'matched\' AND matched_type=\'group\' AND matched_id=?")');
    expect(instruments).toContain('bankTxnGuardSql(" AND match_status=\'matched\' AND matched_type=\'journal\' AND matched_id=?")');
    // Native txnMatch/txnUnmatch/bankUnmatch bindings remain byte-for-byte.
    expect(bank).toContain('.txnMatch([id, b.id])');
    expect(bank).toContain('.txnUnmatch([b])');
    expect(instruments).toContain('.bankUnmatch([run.bank_transaction_id])');
  });
});

describe('Finance read-tail r48 Rank3 acquisition reads and guards', () => {
  test('renders all three acquisition reads byte-identically', () => {
    expect(acquisitionReadSql('*', "id=? AND holding_id=? AND company_id=? AND status='approved' AND currency=? AND acquisition_date<=?")).toBe(ACQ_APPROVED);
    expect(acquisitionReadSql('*', "id=? AND holding_id=? AND investee_entity_id=? AND status IN ('approved','impaired') AND acquisition_date<=? AND currency=? AND calculation_run_id=?")).toBe(ACQ_EFFECT);
    expect(acquisitionReadSql('1', "company_id=? AND ownership_id=? AND acquisition_date=? AND currency=? AND source_hash IS NOT NULL AND status IN ('draft','approved','impaired') AND id NOT IN (?,?)")).toBe(ACQ_SOURCE);
  });

  test('renders both acquisition version guards byte-identically', () => {
    expect(acquisitionVersionGuardSql('approved')).toBe(ACQ_PARENT);
    expect(acquisitionVersionGuardSql('draft')).toBe(ACQ_VERSION);
    expect(acquisitionVersionGuardSql('approved')).not.toBe(ACQ_VERSION);
  });

  test('all five sites delegate with their original binds', () => {
    const editions = read('finance-group-editions.ts');
    const ownership = read('finance-group-ownership.ts');
    expect(count(editions, 'acquisitionReadSql(')).toBe(2);
    expect(count(ownership, 'acquisitionReadSql(')).toBe(1);
    expect(count(ownership, 'acquisitionVersionGuardSql(')).toBe(2);
    for (const src of [editions, ownership]) {
      expect(src).not.toContain(ACQ_APPROVED);
      expect(src).not.toContain(ACQ_EFFECT);
      expect(src).not.toContain(ACQ_SOURCE);
      expect(src).not.toContain(ACQ_PARENT);
      expect(src).not.toContain(ACQ_VERSION);
    }
    expect(editions).toContain('.bind(acquisitionId, holdingId, companyId, currency, endDate)');
    expect(editions).toContain('.bind(value, holdingId, investee_entity_id, endDate, currency, id)');
    expect(ownership).toContain('.bind(companyId, ownershipId, acquisitionDate, currency, value, currentId)');
    expect(ownership).toContain("acquisitionVersionGuardSql('approved'), [");
    expect(ownership).toContain("acquisitionVersionGuardSql('draft'), [id, companyId, holdingId, expectedVersion]");
  });
});

describe('Finance read-tail r48 protected families and retirement', () => {
  test('leaves locks, tax, advisory, native, expansion unions and prior helpers alone', () => {
    expect(read('sales.ts')).toContain('FOR SHARE');
    expect(read('finance-reporting.ts')).toContain('FOR UPDATE');
    expect(read('finance-withholding-remittance.ts')).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(read('finance-tax-register.ts')).toContain('finance_current_tax_entries');
    expect(read('finance.ts')).toContain('nativeFinanceOwnerWrite');
    expect(read('finance-expansion-work.ts')).toContain('UNION ALL');
    expect(read('finance-read-tail-r46-sql.ts')).toContain('accountChartGuardSql');
    expect(read('finance-read-tail-r45-sql.ts')).toContain('versionStatusGuard');
    expect(read('finance-read-tail-r42-sql.ts')).toContain('intercompanyPairPointSql');
  });

  test('each r48 family is defined exactly once', () => {
    const helper = read('finance-read-tail-r48-sql.ts');
    for (const name of ['planEditionPointSql', 'bankTxnGuardSql', 'acquisitionReadSql', 'acquisitionVersionGuardSql']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
  });

  test('retirement:ts-statements stays at or below the r48 target of 1844', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Ten inline SELECTs collapse into three r48 helpers (-7: 1851 -> 1844); r49 collapses 36 more into four helpers (-32: 1844 -> 1812); r50 removes seven more (-> 1805).
    expect(total).toBe(1562);
    expect(total).toBeLessThan(1844);
  });
});
