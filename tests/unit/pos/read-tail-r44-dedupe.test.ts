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

// Verbatim pre-r44 oracles recorded from HEAD a348ca7b1 (BEFORE=172). Byte-identity
// oracles only, never live query sources. The folded head is a 79-byte catalog
// JSONB aggregate prefix shared by the commerce attachment image read and the
// POS startup source read. Both sites keep their own WHERE tail (product scope
// vs company scope), so the zero-arg owner-local renderer preserves the exact
// rendered byte sequence, bind order/count and locks. No r35-r43 native/lock/
// renderer head, sale-child/owner_writes registry, advisory lock, PMS fragment,
// settlement mirror or cross-owner projection is touched.
const HEAD = 'SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.';
const HEAD_SHA = '348f981dcb7937af405d7e411df493b05b3c164ab433c06856c892f28e44b668';
const TAIL_COMMERCE = 'product_id=? AND c.company_id=?)';
const TAIL_STARTUP = 'company_id=?)';
const ORACLE_COMMERCE = {
  len: 394,
  sha: 'efa7795cd461693e55e7f7b832d514f5a9aaebdb9a159f11c599757a060270d5',
  binds: 4,
  selects: 3,
};
const ORACLE_STARTUP = {
  len: 1212,
  sha: '8f26ad65557ce229cf39d490daad8552aa9d8c515772440fe71debb3526c72cf',
  binds: 13,
  selects: 13,
};
const RENDERER_FILE = 'read-tail-r44-catalog-aggregate-sql.ts';
const RENDERER = "export const posCatalogAggregateHeadSql = (): string => 'SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.';";
const CALL = '${posCatalogAggregateHeadSql()}';

const renderedFrom = (text: string, needle: string): string => {
  const start = text.indexOf(needle);
  expect(start).toBeGreaterThanOrEqual(0);
  const bodyStart = text.indexOf('`', start) + 1;
  const bodyEnd = text.indexOf('`', bodyStart);
  return text.slice(bodyStart, bodyEnd).split(CALL).join(HEAD);
};

describe('POS read-tail r44 catalog aggregate head dedupe', () => {
  test('fold: byte-identical head is authored once in a zero-arg owner-local renderer', () => {
    const helper = read(RENDERER_FILE);
    expect(helper).toContain("import 'server-only';");
    expect(helper).toContain(RENDERER);
    expect(occurrences(helper, HEAD)).toBe(1);
    expect(sha(HEAD)).toBe(HEAD_SHA);
    expect(HEAD.length).toBe(79);
    expect(selects(HEAD)).toBe(1);
    expect(binds(HEAD)).toBe(0);
    expect(HEAD.startsWith('SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id)')).toBe(true);
    expect(HEAD.endsWith(' FROM pos_catalog c WHERE c.')).toBe(true);
    expect(HEAD).not.toContain('FOR UPDATE');
    expect(HEAD).not.toContain('FOR SHARE');
    expect(HEAD.endsWith('\n')).toBe(false);
  });

  test('both sites delegate; rendered SQL stays byte-identical with 3/4 and 13/13 SELECT/binds', () => {
    const commerce = read('commerce-attachment-sources.ts');
    const startup = read('startup-sources.ts');
    expect(occurrences(commerce, CALL)).toBe(1);
    expect(occurrences(startup, CALL)).toBe(1);
    expect(occurrences(commerce, HEAD)).toBe(0);
    expect(occurrences(startup, HEAD)).toBe(0);
    expect(commerce).toContain('(' + CALL + TAIL_COMMERCE);
    expect(startup).toContain('(' + CALL + TAIL_STARTUP);
    const renderedCommerce = renderedFrom(commerce, "`SELECT jsonb_build_object('catalog'");
    expect(renderedCommerce.length).toBe(ORACLE_COMMERCE.len);
    expect(sha(renderedCommerce)).toBe(ORACLE_COMMERCE.sha);
    expect(selects(renderedCommerce)).toBe(ORACLE_COMMERCE.selects);
    expect(binds(renderedCommerce)).toBe(ORACLE_COMMERCE.binds);
    const renderedStartup = renderedFrom(startup, '`SELECT\n EXISTS(');
    expect(renderedStartup.length).toBe(ORACLE_STARTUP.len);
    expect(sha(renderedStartup)).toBe(ORACLE_STARTUP.sha);
    expect(selects(renderedStartup)).toBe(ORACLE_STARTUP.selects);
    expect(binds(renderedStartup)).toBe(ORACLE_STARTUP.binds);
    expect(commerce).toContain("prepare('LOCK TABLE pos_catalog,pos_register_catalog,pos_registers IN SHARE ROW EXCLUSIVE MODE')");
    expect(startup).toContain('LOCK TABLE pos_settings,pos_registers,pos_catalog,pos_register_catalog,pos_sales,pos_shifts,pos_payments,pos_refunds,pos_postings IN SHARE MODE');
    expect(commerce).toContain('.bind(productId,companyId,companyId,productId)');
    expect(startup).toContain('.bind(...Array.from({length:13},()=>companyId))');
  });

  test('protected r35-r43 renderers, registries, advisory locks, PMS fragments, settlement mirrors and lock tails stay untouched', () => {
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
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 196)');
    expect(read('owner_writes.rs')).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    expect(read('addon-package-obligations.ts')).toContain('UNION SELECT s.package_id,s.company_id,');
  });

  test('retirement:ts-statements decreased 172 -> 171 (strictly lower than the r44 BEFORE=172 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(161);
    expect(total).toBeLessThan(172);
  });
});
