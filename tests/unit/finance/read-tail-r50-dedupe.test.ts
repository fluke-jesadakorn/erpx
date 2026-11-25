import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  clockProbeSql,
  paymentSlipChecksumGuardSql,
  THAI_DOC_ACTIVE_GUARD_SQL,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r50-sql';
import { tableByIdSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r49-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-finance/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;

describe('Finance read-tail r50 Rank1 clock scalar probes', () => {
  test('renders byte-identically to the retired scalar probes', () => {
    expect(clockProbeSql('P', 'finance_search_clock')).toBe('(SELECT P FROM finance_search_clock)');
    expect(clockProbeSql('Q', 'finance_notice_clock')).toBe('(SELECT Q FROM finance_notice_clock)');
    expect(clockProbeSql("a'b+c?", 'finance_search_clock')).toBe("(SELECT a'b+c? FROM finance_search_clock)");
    expect(clockProbeSql('P', 'finance_search_clock')).not.toBe(clockProbeSql('P', 'finance_notice_clock'));
  });

  test('all four sites delegate; outer EXISTS, CTEs and bind order stay intact', () => {
    const doc = read('document-purchases.ts');
    expect(count(doc, 'clockProbeSql(')).toBe(4);
    expect(doc).toContain("clockProbeSql(purchasedCapabilityPredicate('workspace_records.company_id','finance.accounting','history','finance_search_clock.now_ms'),'finance_search_clock')");
    expect(doc).toContain("clockProbeSql(initialPhysicalStockFinanceJournalFilter('workspace_records','finance_search_clock.now_ms'),'finance_search_clock')");
    expect(doc).toContain('clockProbeSql(purchasedCapabilityPredicate("d.company_id",capability,"history","finance_notice_clock.now_ms"),\'finance_notice_clock\')');
    expect(doc).toContain("clockProbeSql(initialPhysicalStockFinanceJournalFilter('d','finance_notice_clock.now_ms'),'finance_notice_clock')");
    expect(doc).not.toContain('`(SELECT ${purchasedCapabilityPredicate(');
    expect(doc).not.toContain('`(SELECT ${initialPhysicalStockFinanceJournalFilter(');
    expect(doc).not.toContain("(SELECT ${initialPhysicalStockFinanceJournalFilter('d'");
    expect(doc).toContain('EXISTS(SELECT 1 FROM finance_journals d WHERE d.id=${id} AND d.company_id=${company} AND ${clockProbeSql(');
    expect(doc).toContain("materializedClockCteSql('finance_search_clock', packageClockSql)");
    expect(doc).toContain("materializedClockCteSql('finance_notice_clock', packageClockSql)");
    expect(doc).toContain("args:mode==='enforced'?[actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]:[actor.assignmentId,actor.userId,actor.role]");
    expect(doc).toContain('assignmentReadSql(\'finance_notice_scope\',\'finance.read\')');
  });
});

describe('Finance read-tail r50 Rank2 payment-slip checksum guards', () => {
  test('renders byte-identically to the retired checksum guards', () => {
    expect(paymentSlipChecksumGuardSql('U', 'P'))
      .toBe('NOT EXISTS(SELECT 1 FROM finance_payments p U ON used.id=p.payment_slip_id P ON proposed.id=? WHERE p.company_id=? AND used.checksum_sha256=proposed.checksum_sha256)');
    expect(paymentSlipChecksumGuardSql('U1', 'P1')).not.toBe(paymentSlipChecksumGuardSql('U1', 'P2'));
  });

  test('both guards delegate; FOR UPDATE and native writes stay untouched', () => {
    const comm = read('commerce-facts.ts');
    expect(count(comm, 'paymentSlipChecksumGuardSql(')).toBe(2);
    expect(count(comm, 'paymentSlipChecksumGuardSql(filesSnapshotSources().commerce_facts_attachments_used_join,filesSnapshotSources().commerce_facts_attachments_proposed_join)')).toBe(2);
    expect(comm).not.toContain('`NOT EXISTS(SELECT 1 FROM finance_payments p ${filesSnapshotSources()');
    expect(comm).toContain("SELECT id FROM finance_invoices WHERE id=? FOR UPDATE");
    expect(comm).toContain("SELECT id FROM finance_parties WHERE id=? AND company_id=? FOR UPDATE");
    expect(comm).toContain('nativeFinanceOwnerWriteSystem(db');
  });
});

describe('Finance read-tail r50 Rank3 Thai active-document guard', () => {
  test('renders byte-identically to both retired guards', () => {
    expect(THAI_DOC_ACTIVE_GUARD_SQL)
      .toBe("NOT EXISTS(SELECT 1 FROM finance_thai_document_events WHERE document_id=? AND action IN ('cancelled','superseded'))");
  });

  test('both guards delegate; advisory and withholding statements stay intact', () => {
    const thai = read('finance-thai-documents.ts');
    expect(count(thai, 'THAI_DOC_ACTIVE_GUARD_SQL')).toBe(3); // import plus two assertion sites
    expect(count(thai, 'assertion(write, commandId, "current-document", THAI_DOC_ACTIVE_GUARD_SQL, [documentId])')).toBe(1);
    expect(count(thai, "assertion(write, commandId, 'current-document', THAI_DOC_ACTIVE_GUARD_SQL, [original.id])")).toBe(1);
    expect(thai).not.toContain('"NOT EXISTS(SELECT 1 FROM finance_thai_document_events');
    expect(thai).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    expect(thai).toContain('withholding_certificate');
    expect(thai).toContain('tableByIdSql(\'finance_thai_document_changes\')');
  });
});

describe('Finance read-tail r50 Rank4 reporting table-by-id reads', () => {
  test('reuses the r49 helper byte-identically', () => {
    expect(tableByIdSql('finance_statement_packages')).toBe('SELECT * FROM finance_statement_packages WHERE id=? AND company_id=?');
    expect(tableByIdSql('finance_year_closings')).toBe('SELECT * FROM finance_year_closings WHERE id=? AND company_id=?');
  });

  test('both reads delegate while row locks stay', () => {
    const rep = read('finance-reporting.ts');
    expect(count(rep, "tableByIdSql('finance_statement_packages')")).toBe(1);
    expect(count(rep, "tableByIdSql('finance_year_closings')")).toBe(1);
    expect(rep).not.toContain('"SELECT * FROM finance_statement_packages WHERE id=? AND company_id=?"');
    expect(rep).not.toContain('"SELECT * FROM finance_year_closings WHERE id=? AND company_id=?"');
    expect(rep).toContain('SELECT chart_id FROM finance_reporting_revisions WHERE chart_id=? FOR UPDATE');
  });
});

describe('Finance read-tail r50 helper definitions and retirement', () => {
  test('each new r50 family is defined exactly once', () => {
    const helper = read('finance-read-tail-r50-sql.ts');
    for (const name of ['clockProbeSql', 'paymentSlipChecksumGuardSql', 'THAI_DOC_ACTIVE_GUARD_SQL']) {
      expect(count(helper, `export const ${name} =`)).toBe(1);
    }
    expect(read('finance-read-tail-r49-sql.ts')).toContain('export const tableByIdSql =');
  });

  test('retirement:ts-statements reaches the r50 target of 1805', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // Four clock probes, two checksum guards and two Thai guards collapse into
    // three r50 exports; two reporting reads reuse r49 tableByIdSql (-7: 1812 -> 1805).
    // r51 reuses r42 for 17 company lists and adds one active-account helper (-22: 1805 -> 1783).
    // r52 delegates 20 version/status guards to one helper (-19: 1783 -> 1764).
    // r53 delegates 11 column-by-id reads to one helper (-11 literals, +1: 1764 -> 1754).
    expect(total).toBe(1754);
    expect(total).toBeLessThan(1812);
  });
});
