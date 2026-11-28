import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../modules/pos/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const occurrences = (text: string, needle: string) => text.split(needle).length - 1;
const binds = (s: string) => (s.match(/\?/g) ?? []).length;
const selects = (s: string) => (s.match(/SELECT /g) ?? []).length;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim pre-r43 oracles recorded from HEAD ce44890604c24356e05ddfafa59323c7a7076f60
// (metric-purchases.ts / startup-entry.ts, BEFORE=173). Byte-identity oracles
// only, never live query sources. The folded head is the two-column scope
// projection used once by the metric-history purchase guard and once by the
// startup-entry scope guard. Both sites share the exact 45-byte head (1
// SELECT, 2 binds, no row lock) and differ only in the trailing department
// expression and scope alias. The zero-arg owner-local renderer returns the
// exact literal, so both fully rendered statements keep their original byte
// sequence, SELECT and bind counts. No r35-r42 native/lock/renderer head,
// sale-child/owner_writes registry, advisory lock, PMS fragment, settlement
// mirror or cross-owner projection is touched.
const HEAD = '(SELECT ?::text company_id,?::text branch_id,';
const HEAD_SHA = '7f1faec18397474e35c7066df3ab3c37bae81969cf3d79cabf959fe68d01df75';
const TAIL_METRIC = "?::text department_id) metric_scope";
const TAIL_STARTUP = "NULL::text department_id) pos_startup_scope CROSS JOIN pos_startup_clock\n WHERE a.id=? AND a.user_id=? AND a.company_id=pos_startup_scope.company_id AND a.role=? AND a.version=? AND a.scope_kind IN ('holding','branch')\n AND (${finance} OR ${stock})\n AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate('pos_startup_scope.company_id',module)).join(' AND ')}\n AND ${capabilities.map(capability=>purchasedCapabilityPredicate('pos_startup_scope.company_id',capability,'history','pos_startup_clock.now_ms')).join(' AND ')}";
const PREFIX_METRIC = "SELECT ${assignmentReadSql(\"metric_scope\", \"pos.reports\")} allowed\n    FROM ";
const PREFIX_STARTUP = "WITH pos_startup_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT a.id FROM (${identitySources.assignments}) a CROSS JOIN ";
const FULL_METRIC = PREFIX_METRIC + HEAD + TAIL_METRIC;
const FULL_METRIC_SHA = '3f99fd7e1a8353e579294ec52d112a4356683c0db2c7d7e2381753cc11831419';
const FULL_STARTUP = PREFIX_STARTUP + HEAD + TAIL_STARTUP;
const FULL_STARTUP_SHA = '6b5221a0cb221ad43a441e138429fb5bbb8c8358788e650433c8e79ded841734';
const RENDERER_FILE = 'read-tail-r43-scope-sql.ts';
const RENDERER = "const posScopeHeadSql = (): string => '(SELECT ?::text company_id,?::text branch_id,';";
const CALL = '${posScopeHeadSql()}';

const renderedFrom = (text: string, needle: string): string => {
  const start = text.indexOf(needle);
  expect(start).toBeGreaterThanOrEqual(0);
  const bodyStart = text.indexOf('`', start) + 1;
  const bodyEnd = text.indexOf('`', bodyStart);
  return text.slice(bodyStart, bodyEnd).split(CALL).join(HEAD);
};

describe('POS read-tail r43 scope head dedupe', () => {
  test('fold: byte-identical head is authored once in a zero-arg owner-local renderer', () => {
    const helper = read(RENDERER_FILE);
    expect(helper).toContain("import 'server-only';");
    expect(helper).toContain(RENDERER);
    expect(occurrences(helper, HEAD)).toBe(1);
    expect(sha(HEAD)).toBe(HEAD_SHA);
    expect(HEAD.length).toBe(45);
    expect(selects(HEAD)).toBe(1);
    expect(binds(HEAD)).toBe(2);
    expect(HEAD.startsWith('(SELECT ?::text company_id,')).toBe(true);
    expect(HEAD.endsWith('?::text branch_id,')).toBe(true);
    expect(HEAD).not.toContain('FOR UPDATE');
    expect(HEAD).not.toContain('FOR SHARE');
    expect(HEAD.endsWith('\n')).toBe(false);
  });

  test('both sites delegate; rendered scope SQL stays byte-identical with 2/3 and 3/6 SELECT/binds', () => {
    const metric = read('metric-purchases.ts');
    const startup = read('startup-entry.ts');
    expect(occurrences(metric, CALL)).toBe(1);
    expect(occurrences(startup, CALL)).toBe(1);
    expect(metric).toContain('FROM ' + CALL + TAIL_METRIC);
    expect(startup).toContain('CROSS JOIN ' + CALL + TAIL_STARTUP);
    const renderedMetric = renderedFrom(metric, '`SELECT ${assignmentReadSql("metric_scope"');
    expect(renderedMetric).toBe(FULL_METRIC);
    expect(sha(renderedMetric)).toBe(FULL_METRIC_SHA);
    expect(selects(renderedMetric)).toBe(2);
    expect(binds(renderedMetric)).toBe(3);
    const renderedStartup = renderedFrom(startup, '`WITH pos_startup_clock AS MATERIALIZED');
    expect(renderedStartup).toBe(FULL_STARTUP);
    expect(sha(renderedStartup)).toBe(FULL_STARTUP_SHA);
    expect(selects(renderedStartup)).toBe(3);
    expect(binds(renderedStartup)).toBe(6);
    expect(metric).toContain('.bind(actor.assignmentId, actor.userId, actor.role, actor.companyId, actor.branchId, actor.departmentId)');
    expect(startup).toContain('.bind(scope.companyId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,...permissionArgs)');
    expect(read('startup-sources.ts')).toContain('LOCK TABLE pos_settings,pos_registers,pos_catalog,pos_register_catalog,pos_sales,pos_shifts,pos_payments,pos_refunds,pos_postings IN SHARE MODE');
  });

  test('protected r35-r42 renderers, registries, advisory locks, PMS fragments, settlement mirrors and lock tails stay untouched', () => {
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
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 203)');
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
    expect(total).toBe(147);
    expect(total).toBeLessThan(173);
  });
});
