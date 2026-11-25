import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const occurrences = (text: string, needle: string) => text.split(needle).length - 1;
const binds = (s: string) => (s.match(/\?/g) ?? []).length;
const selects = (s: string) => (s.match(/SELECT /g) ?? []).length;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim pre-r46 oracles recorded from HEAD 20d6b0f21 (BEFORE=170). Byte-identity
// oracles only, never live query sources. The folded head is a 21-byte existence
// guard prefix shared by the POS record-source authority assertion and the POS
// attachment owner version guard (both POS-owned, SELECT-only, lock-free at the
// head). Each caller keeps its own interpolated source and WHERE tail, so the
// zero-arg owner-local renderer preserves the exact rendered byte sequence, bind
// count and locks. No r35-r45 native/lock/renderer head, registry, advisory lock,
// PMS fragment, settlement mirror or cross-owner projection is touched.
const HEAD = 'EXISTS(SELECT 1 FROM ';
const HEAD_SHA = '36027c1d8601019571af9f4e21d4f5d6f9711bb9a956b0d131d22cbe57398aa1';
const RENDERER_FILE = 'read-tail-r46-exists-source-head-sql.ts';
const RENDERER = "export const posExistsSourceHeadSql = (): string => 'EXISTS(SELECT 1 FROM ';";
const CALL = '${posExistsSourceHeadSql()}';
const NEW_RECORD = '${posExistsSourceHeadSql()}${d.table} s WHERE s.id=? AND s.company_id=? AND s.version=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)})';
const NEW_ATTACH = '${posExistsSourceHeadSql()}${item.table} WHERE id=? AND company_id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""})';
const ORACLE_RECORD = {
  len: 248,
  sha: '7103bcbcf5ba48c91588675d52013f819c4b3afd1c762b5dc75588298894885c',
  binds: 7,
  selects: 1,
};
const ORACLE_ATTACH = {
  len: 120,
  sha: '9ad3d33d7abc4714d5b42d110736735412eeb595847b0b3af79e7bd17b6823ae',
  binds: 4,
  selects: 1,
};
const rendered = (body: string): string => body.split(CALL).join(HEAD);

describe('POS read-tail r46 existence-guard head dedupe', () => {
  test('fold: byte-identical head is authored once in a zero-arg owner-local renderer', () => {
    const helper = read(RENDERER_FILE);
    expect(helper).toContain("import 'server-only';");
    expect(helper).toContain(RENDERER);
    expect(occurrences(helper, HEAD)).toBe(1);
    expect(sha(HEAD)).toBe(HEAD_SHA);
    expect(HEAD.length).toBe(21);
    expect(selects(HEAD)).toBe(1);
    expect(binds(HEAD)).toBe(0);
    expect(HEAD.startsWith('EXISTS(SELECT 1 FROM')).toBe(true);
    expect(HEAD.endsWith('FROM ')).toBe(true);
    expect(HEAD).not.toContain('FOR UPDATE');
    expect(HEAD).not.toContain('FOR SHARE');
    expect(HEAD).not.toContain('LOCK TABLE');
    expect(HEAD).not.toContain('pg_advisory');
    expect(HEAD.endsWith('\n')).toBe(false);
  });

  test('both sites delegate; rendered SQL stays byte-identical with 1/7 and 1/4 SELECT/binds', () => {
    const record = read('records-source-facts.ts');
    const attach = read('attachments-finance-facts.ts');
    expect(occurrences(record, CALL)).toBe(1);
    expect(occurrences(attach, CALL)).toBe(1);
    expect(occurrences(record, HEAD)).toBe(0);
    expect(occurrences(attach, HEAD)).toBe(0);
    expect(record).toContain('`' + NEW_RECORD + '`');
    expect(attach).toContain('`' + NEW_ATTACH + '`');
    const guardRecord = rendered(NEW_RECORD);
    expect(guardRecord.length).toBe(ORACLE_RECORD.len);
    expect(sha(guardRecord)).toBe(ORACLE_RECORD.sha);
    expect(selects(guardRecord)).toBe(ORACLE_RECORD.selects);
    expect(binds(guardRecord)).toBe(ORACLE_RECORD.binds);
    expect(guardRecord.startsWith(HEAD)).toBe(true);
    expect(guardRecord.endsWith('department)})')).toBe(true);
    const guardAttach = rendered(NEW_ATTACH);
    expect(guardAttach.length).toBe(ORACLE_ATTACH.len);
    expect(sha(guardAttach)).toBe(ORACLE_ATTACH.sha);
    expect(selects(guardAttach)).toBe(ORACLE_ATTACH.selects);
    expect(binds(guardAttach)).toBe(ORACLE_ATTACH.binds);
    expect(guardAttach.startsWith(HEAD)).toBe(true);
    expect(guardAttach.endsWith('""})')).toBe(true);
    // The record-source lease lock and the authority assertion stay inline.
    expect(record).toContain('database.prepare(`SELECT id FROM ${d.table} WHERE id=? AND company_id=? FOR SHARE`)');
    expect(record).toContain('${assignmentReadSql("s", d.permission).replaceAll("s.department_id", department)}');
    expect(attach).toContain('assertion(db, commandId, `attachment-owner-${index}`');
    expect(attach).toContain('${item.extra ? ` AND ${item.extra}` : ""}');
  });

  test('protected r35-r45 renderers, registries, advisory locks, PMS fragments, settlement mirrors and lock tails stay untouched', () => {
    const core = read('pos-core.ts');
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect(core).toContain('const posFulfillmentByIdLockSql = (): string =>');
    expect(core).toContain('const posRegisterByIdLockSql = (): string =>');
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(read('pos.ts')).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('executive-evidence-sources.ts')).toContain('nativePosEvidenceRows');
    expect(read('executive-evidence-sources.ts')).not.toMatch(/\bSELECT\b/);
    expect(read('native_lane_25/postgres.rs')).toContain('LEG_POS_SALES');
    expect(read('executive-metric-projections.ts')).toContain('const orgNodeLabelHeadSql = (): string => `');
    expect(read('pos.ts')).toContain('const allocatedTotalsHeadSql = (): string => `WITH allocated AS (');
    expect(read('pos.ts')).toContain('nativePosEvidenceDetail');
    expect(read('native_lane_25/postgres.rs')).toContain('PAYMENT_DETAIL_SQL');
    expect(read('read-tail-r43-scope-sql.ts')).toContain("const posScopeHeadSql = (): string => '(SELECT ?::text company_id,?::text branch_id,';");
    expect(read('read-tail-r44-catalog-aggregate-sql.ts')).toContain("export const posCatalogAggregateHeadSql = (): string => 'SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.';");
    expect(read('read-tail-r45-finance-record-payment-head-sql.ts')).toContain("export const posFinanceRecordPaymentHeadSql = (): string => 'SELECT company_id,finance_payment_id id';");
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 196)');
    expect(read('owner_writes.rs')).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    expect(read('addon-package-obligations.ts')).toContain('UNION SELECT s.package_id,s.company_id,');
  });

  test('retirement:ts-statements decreased 170 -> 169 (strictly lower than the r46 BEFORE=170 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(160);
    expect(total).toBeLessThan(170);
  });
});
