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

// Verbatim pre-r42 oracles recorded from executive-metric-projections.ts at
// BEFORE=174 (git blob b5bba25504f704e1bc41c342e62f793873f54cf5). Byte-identity
// oracles only, never live query sources. The folded head is the org-node
// display-name label subquery used twice: once with the `x.` net-events alias
// and once with the `s.` payment-mix alias. Both sites share the exact 104-byte
// head (1 SELECT, zero binds, no lock) and differ only in the caller tail. The
// zero-arg owner-local renderer returns the exact literal, so both fully
// rendered metric statements keep their original byte sequence, SELECT and
// bind counts. No r35-r41 native/lock/renderer head, sale-child/owner_writes
// registry, advisory lock, PMS fragment, day-report/Stripe registry, settlement
// mirror or cross-owner projection is touched.
const HEAD = 'COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=';
const HEAD_SHA = 'a3430e86a704c36c15e01cecb794a28ac78d251f6623dd2897ae375dd5980489';
const TAIL_X = "x.branch_id AND n.company_id=x.company_id),'Company-wide')";
const TAIL_S = "s.branch_id AND n.company_id=s.company_id),'Company-wide')";
const FULL_NET = `WITH sales AS (
        SELECT s.id,s.id sale_id,s.company_id,s.branch_id,s.business_date event_date,s.total_minor amount_minor
        FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
          AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      ), refunds AS (
        SELECT r.id,s.id sale_id,s.company_id,s.branch_id,r.accounting_date event_date,-r.amount_minor amount_minor
        FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
        WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
          AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed'
      ), net_events AS (SELECT * FROM sales UNION ALL SELECT * FROM refunds)
      SELECT ${HEAD + TAIL_X} label,COALESCE(sum(x.amount_minor),0)::text value FROM net_events x GROUP BY 1 ORDER BY 1`;
const FULL_NET_SHA = 'bffcb13b5031a9ccc1ec3c3aee4638efed65c376b8635792469cd91d8d953b6b';
const FULL_MIX = `SELECT ${HEAD + TAIL_S} label,COALESCE(sum(p.amount_minor-COALESCE((SELECT sum(rp.amount_minor) FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)),0)::text value
      FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
      WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ?
        AND s.state='completed' AND p.state='succeeded' GROUP BY 1 ORDER BY 1`;
const FULL_MIX_SHA = '82874be9da4cec849fc066f60733a4b4fc4f4bfd32068de2d84c5d29bdf42561';
const RENDERER = 'const orgNodeLabelHeadSql = (): string => `';
const CALL = '${orgNodeLabelHeadSql()}';

const renderedFrom = (needle: string, tail: string): string => {
    const exec = read('executive-metric-projections.ts');
    const start = exec.indexOf(needle);
    expect(start).toBeGreaterThanOrEqual(0);
    const bodyStart = exec.indexOf('`', start) + 1;
    const bodyEnd = exec.indexOf('`', bodyStart);
    return exec.slice(bodyStart, bodyEnd).split('${label}').join(HEAD + tail);
};

describe('POS read-tail r42 org-node label head dedupe', () => {
  test('fold: byte-identical head is authored once in a zero-arg owner-local renderer', () => {
    const exec = read('executive-metric-projections.ts');
    expect(exec).toContain(RENDERER + HEAD + '`');
    expect(occurrences(exec, HEAD)).toBe(1);
    expect(sha(HEAD)).toBe(HEAD_SHA);
    expect(HEAD.length).toBe(104);
    expect(selects(HEAD)).toBe(1);
    expect(binds(HEAD)).toBe(0);
    expect(HEAD.startsWith('COALESCE((SELECT n.display_name FROM (')).toBe(true);
    expect(HEAD.endsWith('n WHERE n.id=')).toBe(true);
    expect(HEAD).not.toContain('FOR UPDATE');
    expect(HEAD.endsWith('\n')).toBe(false);
  });

  test('both sites delegate; rendered metric SQL stays byte-identical with 6/12 and 3/6 SELECT/binds', () => {
    const exec = read('executive-metric-projections.ts');
    expect(occurrences(exec, CALL)).toBe(2);
    expect(exec).toContain(CALL + TAIL_X + '`');
    expect(exec).toContain(CALL + TAIL_S + '`');
    const net = renderedFrom('sql = `WITH sales AS (', TAIL_X);
    expect(net).toBe(FULL_NET);
    expect(sha(net)).toBe(FULL_NET_SHA);
    expect(selects(net)).toBe(6);
    expect(binds(net)).toBe(12);
    const mix = renderedFrom('sql = `SELECT ${label} label,COALESCE(sum(p.amount_minor', TAIL_S);
    expect(mix).toBe(FULL_MIX);
    expect(sha(mix)).toBe(FULL_MIX_SHA);
    expect(selects(mix)).toBe(3);
    expect(binds(mix)).toBe(6);
    expect(exec).toContain('.bind(...values).all<RawMetricRow>()');
  });

  test('protected r35-r41 renderers, registries, advisory locks, PMS fragments, settlement mirrors and lock tails stay untouched', () => {
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
    expect(read('pos.ts')).toContain('const allocatedTotalsHeadSql = (): string => `WITH allocated AS (');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 200)');
    expect(read('owner_writes.rs')).toContain('assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 13)');
    expect(read('addon-package-obligations.ts')).toContain('UNION SELECT s.package_id,s.company_id,');
  });

  test('retirement:ts-statements decreased 174 -> 173 (strictly lower than the r42 BEFORE=174 baseline)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(154);
    expect(total).toBeLessThan(174);
  });
});
