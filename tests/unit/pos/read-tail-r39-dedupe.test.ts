import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const occurrences = (text: string, needle: string) => text.split(needle).length - 1;
const binds = (s: string) => (s.match(/\?/g) ?? []).length;
const selects = (s: string) => (s.match(/SELECT /g) ?? []).length;

// Verbatim pre-r39 literals recorded from the ASTs at BEFORE=179
// (HEAD 3b6af4436d56ae81ca5628b967c2b59676d9e9a4). Byte-identity oracles only,
// never live query sources. The allocated head stops exactly at `GROUP BY
// sale_id)` (no trailing newline, no `, sources AS (`), so the L3 SELECT tail
// and the L4 sources CTE tail keep their pms_sources fragments and 4/9 binds.
// The sale leg stops at `>0)` (no trailing newline) so the net-pos-sales
// UNION ALL refund leg is unchanged. No Rust registry, sale-child registry,
// PMS fragment, advisory lock or cross-owner settlement mirror is touched.
const ALLOCATED_HEAD = `WITH allocated AS (
 SELECT l.*,f.state fulfillment_state,
 ROUND(f.cost_minor::numeric*SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)/f.quantity_milli)
 -ROUND(f.cost_minor::numeric*(SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)-l.quantity_milli)/f.quantity_milli) cost_share
 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE f.company_id=?
 ), totals AS (SELECT sale_id,SUM(refunded_minor) refunds_minor,SUM(refunded_tax_minor) refunded_tax_minor,
 SUM(CASE WHEN fulfillment_state='delivered' THEN cost_share ELSE 0 END-returned_cost_minor) cost_minor FROM allocated GROUP BY sale_id)`;
const POS_SALES_LEG = `SELECT 'sale:'||s.id id,s.id source_id,'sale' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,s.state status,s.state sale_status,s.currency,NULL::text method,
      s.total_minor::numeric amount_minor,s.total_minor::numeric contribution_minor,0::numeric refunded_minor,
      s.version revision,s.version sale_version
    FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)`;

const HEAD_RENDERER = 'const allocatedTotalsHeadSql = (): string => ';
const LEG_RENDERER = 'const posSalesLegSql = (): string => ';

describe('POS read-tail r39 allocated-totals head and evidence sale-leg dedupe', () => {
  test('fold 1: byte-identical head is authored once in a zero-arg owner-local renderer', () => {
    const pos = read('pos.ts');
    expect(pos).toContain(HEAD_RENDERER + '`' + ALLOCATED_HEAD + '`');
    expect(occurrences(pos, ALLOCATED_HEAD)).toBe(1);
    expect(binds(ALLOCATED_HEAD)).toBe(1);
    expect(selects(ALLOCATED_HEAD)).toBe(2);
    expect(ALLOCATED_HEAD.endsWith('GROUP BY sale_id)')).toBe(true);
    expect(ALLOCATED_HEAD).not.toContain(', sources AS (');
    expect(ALLOCATED_HEAD.endsWith('\n')).toBe(false);
  });

  test('fold 1: both sites delegate; resolved SQL keeps 4/9 binds and pms_sources tails', () => {
    const pos = read('pos.ts');
    expect(occurrences(pos, '${allocatedTotalsHeadSql()}')).toBe(2);
    const l3 = pos.slice(pos.indexOf('readPosLines3:'), pos.indexOf('readPosRefundPayments2:'));
    const l4 = pos.slice(pos.indexOf('readPosLines4:'), pos.indexOf('posCustomers:'));
    expect(l3).toContain('${allocatedTotalsHeadSql()}\n SELECT s.business_date');
    expect(l4).toContain('${allocatedTotalsHeadSql()}, sources AS (');
    expect(binds(l3.replace('${allocatedTotalsHeadSql()}', ALLOCATED_HEAD))).toBe(4);
    expect(binds(l4.replace('${allocatedTotalsHeadSql()}', ALLOCATED_HEAD))).toBe(9);
    expect(occurrences(pos, '${pmsProjectionSources.pms_sources}')).toBe(3);
  });

  test('fold 2: the byte-identical sale leg is authored once in the native owner (lane 25)', () => {
    const rust = read('native_lane_25/postgres.rs');
    const ev = read('executive-evidence-sources.ts');
    expect(occurrences(rust, POS_SALES_LEG)).toBe(1);
    expect(binds(POS_SALES_LEG)).toBe(8);
    expect(selects(POS_SALES_LEG)).toBe(1);
    expect(POS_SALES_LEG.endsWith('>0)')).toBe(true);
    expect(POS_SALES_LEG.endsWith('\n')).toBe(false);
    expect(rust).toContain('LEG_REFUND');
    expect(rust).toContain('UNION ALL');
    expect(ev).not.toContain('posSalesLegSql');
    expect(ev).toContain('nativePosEvidenceRows');
    expect(ev).not.toMatch(/\bSELECT\b/);
  });

  test('protected r38 lock renderer, sale-child registry, PMS fragments, advisory locks and settlement mirrors stay untouched', () => {
    const core = read('pos-core.ts');
    expect(core).toContain("const posSaleByIdLockSql = (): string => 'SELECT * FROM pos_sales WHERE id=? FOR UPDATE';");
    expect((core.match(/FOR UPDATE/g) ?? []).length).toBe(17);
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(read('pos.ts')).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('pos-stripe.ts')).toContain('SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE');
    expect(read('owner_writes.rs')).toContain('assert_eq!(KINDS.len(), 203)');
  });

  test('retirement:ts-statements decreased 179 -> 173 (r42 continues below the r39 BEFORE=179 baseline)', () => {
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
    expect(total).toBeLessThan(179);
  });
});
