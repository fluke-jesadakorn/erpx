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

// Verbatim pre-r45 oracles recorded from HEAD cd8870ccb0 (BEFORE=171). Byte-identity
// oracles only, never live query sources. The folded head is a 39-byte payment
// identity prefix shared by the retained Finance payment source and its opaque
// source-image projection (both POS-owned, SELECT-only, lock-free). Each caller
// keeps its own FROM/WHERE or to_jsonb tail, so the zero-arg owner-local renderer
// preserves the exact rendered byte sequence, bind count and locks. No r35-r44
// native/lock/renderer head, registry, advisory lock, PMS fragment, settlement
// mirror or cross-owner projection is touched.
const HEAD = 'SELECT company_id,finance_payment_id id';
const HEAD_SHA = 'b152c819bf5e8b8e126389e7666a0460da82f17825031dd1b805f34dfee7935b';
const TAIL_SOURCE = ' FROM pos_payments WHERE finance_payment_id IS NOT NULL';
const TAIL_IMAGE = ',to_jsonb(source) source_image FROM pos_payments source WHERE finance_payment_id IS NOT NULL';
const ORACLE_SOURCE = {
  len: 94,
  sha: 'ce30f681fb2a9c5c4acfe8dab54aad2de5360f61f78b6b74dda22c7b6195038e',
  binds: 0,
  selects: 1,
};
const ORACLE_IMAGE = {
  len: 131,
  sha: 'ffb173b971b7debc0dcb0ebbfebd780753f0456c2b29f49698837fd8c789ec92',
  binds: 0,
  selects: 1,
};
const RENDERER_FILE = 'read-tail-r45-finance-record-payment-head-sql.ts';
const RENDERER = "export const posFinanceRecordPaymentHeadSql = (): string => 'SELECT company_id,finance_payment_id id';";
const CALL = '${posFinanceRecordPaymentHeadSql()}';

const renderedAt = (text: string, needle: string): string => {
  const at = text.indexOf(needle);
  expect(at).toBeGreaterThanOrEqual(0);
  const bodyStart = text.lastIndexOf('`', at) + 1;
  const bodyEnd = text.indexOf('`', at);
  return text.slice(bodyStart, bodyEnd).split(CALL).join(HEAD);
};

describe('POS read-tail r45 finance payment identity head dedupe', () => {
  test('fold: byte-identical head is authored once in a zero-arg owner-local renderer', () => {
    const helper = read(RENDERER_FILE);
    expect(helper).toContain("import 'server-only';");
    expect(helper).toContain(RENDERER);
    expect(occurrences(helper, HEAD)).toBe(1);
    expect(sha(HEAD)).toBe(HEAD_SHA);
    expect(HEAD.length).toBe(39);
    expect(selects(HEAD)).toBe(1);
    expect(binds(HEAD)).toBe(0);
    expect(HEAD.startsWith('SELECT company_id,finance_payment_id')).toBe(true);
    expect(HEAD.endsWith(' id')).toBe(true);
    expect(HEAD).not.toContain('FOR UPDATE');
    expect(HEAD).not.toContain('FOR SHARE');
    expect(HEAD).not.toContain('LOCK TABLE');
    expect(HEAD).not.toContain('pg_advisory');
    expect(HEAD.endsWith('\n')).toBe(false);
  });

  test('both sites delegate; rendered SQL stays byte-identical with 1/0 SELECT/binds', () => {
    const sources = read('finance-record-sources.ts');
    expect(occurrences(sources, CALL)).toBe(2);
    expect(occurrences(sources, HEAD)).toBe(0);
    expect(sources).toContain('`' + CALL + TAIL_SOURCE + '`');
    expect(sources).toContain('`' + CALL + TAIL_IMAGE + '`');
    const source = renderedAt(sources, TAIL_SOURCE);
    expect(source.length).toBe(ORACLE_SOURCE.len);
    expect(sha(source)).toBe(ORACLE_SOURCE.sha);
    expect(selects(source)).toBe(ORACLE_SOURCE.selects);
    expect(binds(source)).toBe(ORACLE_SOURCE.binds);
    expect(source.startsWith(HEAD)).toBe(true);
    expect(source.endsWith('finance_payment_id IS NOT NULL')).toBe(true);
    const image = renderedAt(sources, TAIL_IMAGE);
    expect(image.length).toBe(ORACLE_IMAGE.len);
    expect(sha(image)).toBe(ORACLE_IMAGE.sha);
    expect(selects(image)).toBe(ORACLE_IMAGE.selects);
    expect(binds(image)).toBe(ORACLE_IMAGE.binds);
    expect(image.startsWith(HEAD)).toBe(true);
    expect(image.endsWith('finance_payment_id IS NOT NULL')).toBe(true);
    expect(source).not.toContain('to_jsonb');
    expect(image).toContain('to_jsonb(source) source_image');
    // The source lease lock and the adjacent settlement mirrors stay inline.
    expect(sources).toContain("prepare('LOCK TABLE pos_sales,pos_postings,pos_settlements,pos_payments IN SHARE ROW EXCLUSIVE MODE')");
    expect(sources).toContain('invoice: "SELECT company_id,invoice_id id FROM pos_sales WHERE invoice_id IS NOT NULL"');
    expect(sources).toContain('journal: "SELECT company_id,journal_id id FROM pos_postings WHERE journal_id IS NOT NULL UNION SELECT company_id,journal_id FROM pos_settlements WHERE journal_id IS NOT NULL"');
    expect(sources).toContain('journal:"SELECT company_id,journal_id id,to_jsonb(source) source_image FROM pos_postings source UNION ALL SELECT company_id,journal_id,to_jsonb(source) FROM pos_settlements source WHERE journal_id IS NOT NULL"');
  });

  test('protected r35-r44 renderers, registries, advisory locks, PMS fragments, settlement mirrors and lock tails stay untouched', () => {
    const core = read('pos-core.ts');
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect(core).toContain('const posFulfillmentByIdLockSql = (): string =>');
    expect(core).toContain('const posRegisterByIdLockSql = (): string =>');
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(read('pos.ts')).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('executive-evidence-sources.ts')).toContain('const refundPaymentSumSql = (): string => `');
    expect(read('executive-evidence-sources.ts')).toContain('const posSalesLegSql = (): string => `');
    expect(read('executive-metric-projections.ts')).toContain('const orgNodeLabelHeadSql = (): string => `');
    expect(read('pos.ts')).toContain('const allocatedTotalsHeadSql = (): string => `WITH allocated AS (');
    expect(read('pos.ts')).toContain('const refundPaymentSumTextSql = (): string => `');
    expect(read('read-tail-r43-scope-sql.ts')).toContain("const posScopeHeadSql = (): string => '(SELECT ?::text company_id,?::text branch_id,';");
    expect(read('read-tail-r44-catalog-aggregate-sql.ts')).toContain("export const posCatalogAggregateHeadSql = (): string => 'SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.';");
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 196)');
    expect(read('owner_writes.rs')).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    expect(read('addon-package-obligations.ts')).toContain('UNION SELECT s.package_id,s.company_id,');
  });

  test('retirement:ts-statements decreased 171 -> 170 (strictly lower than the r45 BEFORE=171 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(170);
    expect(total).toBeLessThan(171);
  });
});
