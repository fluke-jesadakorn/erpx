import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  reservedStockAggSql,
  qualityHeldExistsSql,
  initialPhysicalCountImageSql,
} from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
}

// Lane-18 cutover retired the POS stock snapshot from TypeScript: the axum
// owner now owns reservedStockAggSql and qualityHeldExistsSql for that read, so
// pos-facts.ts is no longer a composed-SQL caller.
const reservedOwners = [
  ['commerce-facts.ts', 2],
  ['commerce-page-sources.ts', 1],
  ['commerce-joined-facts.ts', 1],
] as const;
const qualityOwners = [
  ['commerce-facts.ts', 2],
] as const;
const imageOwners = [
  'manufacturing-initial-physical-stock-count-source.ts',
  'pos-initial-physical-stock-count-source.ts',
] as const;

describe('Inventory read-tail r17 parameterized agg/EXISTS-family dedupe', () => {
  test('canonical helpers render the exact byte-identical SQL and stay read-only', () => {
    expect(reservedStockAggSql('r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id'))
      .toBe('COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0)');
    expect(reservedStockAggSql('r.order_line_id=l.id'))
      .toBe('COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.order_line_id=l.id),0)');
    expect(qualityHeldExistsSql('HOLDS', "h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status NOT IN ('released','cancelled','disposed')"))
      .toBe("CASE WHEN EXISTS(SELECT 1 FROM (HOLDS) h WHERE h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status NOT IN ('released','cancelled','disposed')) THEN 1 ELSE 0 END quality_held");
    expect(qualityHeldExistsSql('HOLDS', "h.status<>'released'"))
      .toBe("CASE WHEN EXISTS(SELECT 1 FROM (HOLDS) h WHERE h.status<>'released') THEN 1 ELSE 0 END quality_held");
    expect(initialPhysicalCountImageSql('IMAGES'))
      .toBe('SELECT image FROM (IMAGES) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))');
    for (const sql of [
      reservedStockAggSql('r.order_line_id=l.id'),
      qualityHeldExistsSql('HOLDS', "h.status<>'released'"),
      initialPhysicalCountImageSql('IMAGES'),
    ]) readOnly(sql);
  });

  test('each r17 helper literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE ${predicate}),0)')).toHaveLength(2);
    expect(shared.split('CASE WHEN EXISTS(SELECT 1 FROM (${holds}) h WHERE ${predicate}) THEN 1 ELSE 0 END quality_held')).toHaveLength(2);
    expect(shared.split('SELECT image FROM (${images}) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))')).toHaveLength(2);
  });

  test('active-reservation SUM is composed, never inlined, in every named adapter', () => {
    const inline = 'COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE ';
    for (const [name] of reservedOwners) expect(src(name), name).not.toContain(inline);
    for (const [name, calls] of reservedOwners) {
      expect(src(name).match(/reservedStockAggSql\(/g) ?? [], name).toHaveLength(calls);
    }
  });

  test('quality_held EXISTS guard is composed, never inlined, in every named adapter', () => {
    const inline = 'CASE WHEN EXISTS(SELECT 1 FROM (${qualityProjectionSources.inventory_quality_holds}) h WHERE ';
    for (const [name] of qualityOwners) expect(src(name), name).not.toContain(inline);
    for (const [name, calls] of qualityOwners) {
      expect(src(name).match(/qualityHeldExistsSql\(/g) ?? [], name).toHaveLength(calls);
    }
  });

  test('initial-count image EXISTS probe is composed, never inlined, in both source adapters', () => {
    const inline = 'EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))';
    for (const name of imageOwners) {
      const text = src(name);
      expect(text, name).not.toContain(inline);
      expect(text.match(/initialPhysicalCountImageSql\(/g) ?? [], name).toHaveLength(1);
    }
  });

  test('retirement:ts-statements stays strictly below the r17 BEFORE=416 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(416);
  });
});
