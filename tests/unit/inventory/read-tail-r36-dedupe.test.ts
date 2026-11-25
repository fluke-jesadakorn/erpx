import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  reservedWarehouseProductQtySql,
  stockPositionSourcesSql,
  inventoryMovementEvidenceRowsSql,
  executivePositionPointSql,
  stockSourceColumnsSql,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Sentinel projection rows. The helpers only dedupe literal SQL text; every
// caller still supplies its own real rows and binds. Using sentinels here keeps
// the byte-identity oracle independent of the live commerce projection.
const W = '<WAREHOUSES>';
const P = '<PRODUCTS>';
const R = reservedWarehouseProductQtySql;
const cond = '<CONDITIONS>';

// Verbatim pre-dedupe literals from HEAD, used strictly as byte-identity
// oracles (never as live query sources).
const headAvailablePositions = `WITH reservations AS (
    ${R}
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    s.quantity_milli::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    s.version stock_version
  FROM commerce_stock s JOIN (${W}) w ON w.id=s.warehouse_id AND w.company_id=? AND w.status='active'
  JOIN (${P}) p ON p.id=s.product_id AND p.company_id=w.company_id AND p.status='active'
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE (?::text IS NULL OR w.branch_id=?)`;
const headStockedPositions = `WITH reservations AS (
    ${R}
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    s.quantity_milli::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    s.version stock_version
  FROM commerce_stock s JOIN (${W}) w ON w.id=s.warehouse_id AND w.company_id=?
  JOIN (${P}) p ON p.id=s.product_id AND p.company_id=w.company_id
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE s.quantity_milli>0`;
const headMovementRows = `SELECT l.id,l.source_type,l.source_id,l.source_number,l.movement_date,l.quantity_milli::text quantity_milli,p.unit,p.sku,p.name product_name,w.id warehouse_id,w.name warehouse_name
    FROM inventory_ledger l JOIN (${W}) w ON w.id=l.warehouse_id JOIN (${P}) p ON p.id=l.product_id AND p.company_id=w.company_id`;
const headSearchTail = `
    WHERE ${cond} ORDER BY l.movement_date DESC,l.id DESC LIMIT ?`;
const headPointTail = `
    WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?) AND l.movement_date BETWEEN ? AND ? AND l.quantity_milli<>0 AND l.id=?`;

describe('Inventory read-tail r36 dedupe', () => {
  test('f1: stockPositionSourcesSql reproduces both position CTE literals byte-identically', () => {
    const available = stockPositionSourcesSql(W, ` AND w.status='active'
  JOIN (${P}) p ON p.id=s.product_id AND p.company_id=w.company_id AND p.status='active'
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE (?::text IS NULL OR w.branch_id=?)`);
    const stocked = stockPositionSourcesSql(W, `
  JOIN (${P}) p ON p.id=s.product_id AND p.company_id=w.company_id
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE s.quantity_milli>0`);
    expect(available).toBe(headAvailablePositions);
    expect(stocked).toBe(headStockedPositions);
    expect(selectCount(available)).toBe(selectCount(headAvailablePositions));
    expect(available).not.toMatch(lockRe);
    expect(stocked).not.toMatch(lockRe);
  });

  test('f2: inventoryMovementEvidenceRowsSql reproduces the search and point projections byte-identically', () => {
    expect(inventoryMovementEvidenceRowsSql(W, P, headSearchTail)).toBe(headMovementRows + headSearchTail);
    expect(inventoryMovementEvidenceRowsSql(W, P, headPointTail)).toBe(headMovementRows + headPointTail);
    expect(inventoryMovementEvidenceRowsSql(W, P, headSearchTail)).not.toMatch(lockRe);
  });

  test('f3: executivePositionPointSql reproduces both point probes byte-identically', () => {
    const availableProbe = executivePositionPointSql(headAvailablePositions, 'available_milli::numeric>0');
    const stockedProbe = executivePositionPointSql(headStockedPositions, 'on_hand_milli::numeric>0');
    expect(availableProbe).toBe(`SELECT * FROM (${headAvailablePositions}) stock WHERE available_milli::numeric>0 AND warehouse_id=? AND product_id=?`);
    expect(stockedProbe).toBe(`SELECT * FROM (${headStockedPositions}) stock WHERE on_hand_milli::numeric>0 AND warehouse_id=? AND product_id=?`);
    expect(availableProbe).not.toMatch(lockRe);
    expect(stockedProbe).not.toMatch(lockRe);
  });

  test('f4: stockSourceColumnsSql reproduces the product and warehouse source literals byte-identically', () => {
    expect(stockSourceColumnsSql('tracking_mode', 'commerce_products')).toBe('SELECT id,company_id,tracking_mode FROM commerce_products');
    expect(stockSourceColumnsSql('branch_id', 'commerce_warehouses')).toBe('SELECT id,company_id,branch_id FROM commerce_warehouses');
    expect(stockSourceColumnsSql('tracking_mode', 'commerce_products')).not.toMatch(lockRe);
  });

  test('each r36 helper template carries exactly one SELECT token and is defined once', () => {
    const shared = src('read-tail-sql.ts');
    for (const name of [
      'stockPositionSourcesSql', 'inventoryMovementEvidenceRowsSql',
      'executivePositionPointSql', 'stockSourceColumnsSql',
    ]) {
      expect(shared.split(`export const ${name}`).length - 1).toBe(1);
      const from = shared.indexOf(`export const ${name}`);
      const rest = shared.slice(from);
      const end = rest.search(/\nexport const |\n\/\*\*/);
      const block = end === -1 ? rest : rest.slice(0, end);
      expect(selectCount(block)).toBe(1);
      expect(block).not.toMatch(lockRe);
    }
  });

  test('former inline literals are gone and call sites compose the helpers with unchanged binds', () => {
    const em = src('executive-main-evidence-sources.ts');
    expect(em).not.toContain('const availableStockPositions = `WITH reservations AS (');
    expect(em).not.toContain('const stockedLocationPositions = `WITH reservations AS (');
    expect(em).not.toContain('`SELECT * FROM (${availableStockPositions})');
    expect(em).not.toContain('`SELECT * FROM (${stockedLocationPositions})');
    expect(em).not.toContain('`SELECT l.id,l.source_type,l.source_id,l.source_number');
    expect(em).toContain('stockPositionSourcesSql(commerceProjectionSources.commerce_warehouses,');
    expect(em).toContain('inventoryMovementEvidenceRowsSql(commerceProjectionSources.commerce_warehouses, commerceProjectionSources.commerce_products,');
    expect(em).toContain("executivePositionPointSql(availableStockPositions, 'available_milli::numeric>0')");
    expect(em).toContain("executivePositionPointSql(stockedLocationPositions, 'on_hand_milli::numeric>0')");
    expect(em).toContain('ORDER BY l.movement_date DESC,l.id DESC LIMIT ?');
    expect(em).toContain('l.quantity_milli<>0 AND l.id=?');

    const ms = src('maintenance-settlement-sources.ts');
    expect(ms).not.toContain('"SELECT id,company_id,tracking_mode FROM commerce_products"');
    expect(ms).not.toContain('"SELECT id,company_id,branch_id FROM commerce_warehouses"');
    expect(ms).toContain("stockSourceColumnsSql('tracking_mode','commerce_products')");
    expect(ms).toContain("stockSourceColumnsSql('branch_id','commerce_warehouses')");
    expect(ms).toContain('"SELECT id,company_id,product_id,tracking_mode FROM inventory_lots"');
  });

  test('r11-r35 helpers, locks, count sources/adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql',
      'clockScopedPurchaseExistsSql', 'lotBalanceGuardSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(src('count-record-purchases.ts')).toContain("clockCteSql('count_source_clock')");
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT status FROM inventory_counts');
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    // The maintenance lock statement is untouched.
    expect(src('maintenance-settlement-sources.ts')).toContain(
      'LOCK TABLE operational_inventory_consumptions,operational_inventory_consumption_lots,inventory_lots,inventory_lot_moves,commerce_products,commerce_warehouses IN SHARE MODE',
    );
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r35 BEFORE=347 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r35 ended at 347. r36 folds four byte-identical families into one helper
    // each (347 -> 343): available/stocked CTE anchor, movement evidence rows,
    // position point probe and maintenance stock source columns.
    expect(total).toBeLessThanOrEqual(343);
    expect(total).toBeLessThan(347);
  });
});
