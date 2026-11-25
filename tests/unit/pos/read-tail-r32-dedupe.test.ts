import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const srcRoot = resolve(__dirname, '../../../packages/server-pos/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');

// Verbatim pre-r32 literals recorded from the AST of `pos-core.ts` at BEFORE=191
// (source SHA b178572c5096e73d3dc4a220b612fb1047358d33). These are byte-identity
// oracles only, never live query sources. Both are Company-scoped, SELECT-only
// and lock-free; they resolve a POS register's property/branch pair directly or
// through a sale's lines/fulfillment for the cross-owner PMS day assertion.
const SQL = [
  'SELECT property_id,branch_id FROM pos_registers WHERE company_id=? AND id=?',
  'SELECT DISTINCT r.property_id,r.branch_id FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND l.fulfillment_id=?',
] as const;
const BINDS = [2, 2] as const;

describe('POS read-tail r32 hotel property native cutover', () => {
  test('native registry holds every retired literal byte-identically with binds 2/2', () => {
    const registry = read('hotel_reads.rs');
    for (let i = 0; i < SQL.length; i++) {
      expect(registry).toContain(`"${SQL[i]}"`);
      expect((SQL[i].match(/\?/g) ?? []).length).toBe(BINDS[i]);
    }
    expect((registry.match(/pub const KINDS: &\[&str\] = &\[/g) ?? []).length).toBe(1);
    expect((registry.match(/pos_hotel_/g) ?? []).length).toBeGreaterThanOrEqual(2);
  });

  test('owner registry mounts the two new kinds with a lock-free SQL arm', () => {
    const owner = read('owner_writes.rs');
    expect(owner).toContain('crate::adapters::hotel_reads::REGISTER_PROPERTY');
    expect(owner).toContain('crate::adapters::hotel_reads::FULFILLMENT_PROPERTY');
    expect(owner).toContain('crate::adapters::hotel_reads::supported(kind)');
    expect(owner).toContain('crate::adapters::hotel_reads::sql(kind)');
    expect(owner).toContain('assert_eq!(KINDS.len(), 194)');
  });

  test('former inline literals are gone and both call sites delegate', () => {
    const core = read('pos-core.ts');
    for (const sql of SQL) expect(core).not.toContain(sql);
    expect(core.match(/nativePos(RegisterProperty|FulfillmentProperty)Statement\(db/g)).toHaveLength(2);
  });

  test('row locks, PMS fragments and day-report/Stripe registries stay inline', () => {
    const core = read('pos-core.ts');
    const pos = read('pos.ts');
    expect(core).toContain('SELECT * FROM pos_sales WHERE id=? FOR UPDATE');
    expect(core).toContain('FOR UPDATE OF rp,r');
    expect(core).toContain("SELECT pg_advisory_xact_lock(hashtextextended('pos-daily:'||?,0))");
    expect(core).toContain('${pmsProjectionSources.pms_properties}');
    expect(pos).toContain('${pmsProjectionSources.pms_sources}');
    expect(read('native-pos-day-report.ts')).toContain("'pos_day_report_");
    expect(read('native-pos-stripe-reads.ts')).toContain("'pos_stripe_");
    expect(read('list_reads.rs')).toContain('pos_list_sales');
  });

  test('retirement:ts-statements is strictly lower than the r32 BEFORE=191 baseline (expected 184)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(184);
    expect(total).toBeLessThan(191);
  });
});
