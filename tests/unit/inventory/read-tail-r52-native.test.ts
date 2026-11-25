import { describe, it, expect, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { INVENTORY_RD_LANDED_COST_DETAIL_LINES } from '../../../packages/server-inventory/src/adapters/native-inventory-read';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

/**
 * Byte oracles captured verbatim from HEAD before retirement. The TypeScript
 * landed-cost detail-lines read was a single-token template: the only
 * interpolation was the Commerce-owned `lineProduct` join fragment.
 */
const HEAD_TEMPLATE =
  'SELECT l.*,p.sku,p.name,p.unit,p.tracking_mode FROM inventory_landed_cost_lines l ${inventorycommerceProjection.lineProduct} WHERE l.landed_cost_id=? ORDER BY l.ordinal';
const LINE_PRODUCT = 'JOIN commerce_products p ON p.id=l.product_id';
const EXPECTED_SQL =
  'SELECT l.*,p.sku,p.name,p.unit,p.tracking_mode FROM inventory_landed_cost_lines l JOIN commerce_products p ON p.id=l.product_id WHERE l.landed_cost_id=? ORDER BY l.ordinal';
const KIND = 'inventory_rd_landed_cost_detail_lines';

describe('r52 landed-cost detail-lines native read', () => {
  it('single-token template resolves byte-identically to the expected SQL with one bind', () => {
    const projection = src('../../server-commerce/src/adapters/inventory-read-projections.ts');
    expect(projection).toContain(`lineProduct: "${LINE_PRODUCT}"`);
    expect(HEAD_TEMPLATE.replace('${inventorycommerceProjection.lineProduct}', LINE_PRODUCT)).toBe(EXPECTED_SQL);
    expect(selectCount(EXPECTED_SQL)).toBe(1);
    expect(EXPECTED_SQL.match(/\?/g)?.length).toBe(1);
  });

  it('registered native kind is byte-identical to the retired TypeScript literal', () => {
    const rust = src('adapters/native_command_sql.rs');
    const m = rust.match(/const SQL_RD_LANDED_COST_DETAIL_LINES: &str =\s*(?:\n\s*)?"((?:[^"\\]|\\.)*)"/);
    expect(m, 'SQL_RD_LANDED_COST_DETAIL_LINES').not.toBeNull();
    expect(m![1].replace(/\\"/g, '"')).toBe(EXPECTED_SQL);
    expect(rust).toContain(`pub const RD_LANDED_COST_DETAIL_LINES: &str = "${KIND}";`);
    const block = rust.match(/const READ_LANDED_COST_DETAIL_LINES: NativeStatement = NativeStatement \{([\s\S]*?)\};/);
    expect(block, 'READ_LANDED_COST_DETAIL_LINES').not.toBeNull();
    expect(block![1]).toContain('kind: RD_LANDED_COST_DETAIL_LINES,');
    expect(block![1]).toContain('sql: SQL_RD_LANDED_COST_DETAIL_LINES,');
    expect(block![1]).toContain('bind_count: 1');
    expect(block![1]).toContain('scope: "holding"');
  });

  it('kind is admitted as a holding-scoped read with one bind and a public re-export', () => {
    const rust = src('adapters/native_command_sql.rs');
    expect(rust).toContain('| RD_LANDED_COST_DETAIL_LINES');
    expect(rust).toContain('RD_LANDED_COST_DETAIL_LINES => Some(&READ_LANDED_COST_DETAIL_LINES),');
    const reexport = src('public/native_command.rs');
    expect(reexport).toContain('RD_LANDED_COST_DETAIL_LINES');
    expect(INVENTORY_RD_LANDED_COST_DETAIL_LINES).toBe(KIND);
  });

  it('TS adapter retired the lines read and keeps the caller decode/Promise.all shape', () => {
    const detail = src('adapters/inventory-landed-costs.ts');
    expect(detail).not.toContain(HEAD_TEMPLATE);
    expect(detail).not.toContain(EXPECTED_SQL);
    expect(detail).not.toContain('${inventorycommerceProjection.lineProduct} ORDER BY l.ordinal');
    expect(detail).toContain('INVENTORY_RD_LANDED_COST_DETAIL_LINES,[id],write');
    expect(detail).toContain('Promise.all([nativeInventoryReadAll<LandedCostRow>(INVENTORY_RD_LANDED_COST_DETAIL_LINES');
    expect(detail).toContain('lines:lines.results,lots:lots.results,events:events.results');
    expect(detail).toContain('requireLandedCostPurchasedRead(write,actor,id)');
    const bridge = src('adapters/native-inventory-read.ts');
    expect(bridge).toContain(`INVENTORY_RD_LANDED_COST_DETAIL_LINES = '${KIND}'`);
    expect(bridge).not.toContain(EXPECTED_SQL);
  });

  it('retirement:ts-statements KPI is 300 after the r53 count-list head fold (strictly below r52 302)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r49 ended at 307; r50 retired 2 landed-cost detail child reads: 307 -> 305;
    // r51 retired 2 transit detail-lot reads: 305 -> 303;
    // r52 retires the single landed-cost detail-lines read: 303 -> 302.
    // r53 folds 3 byte-identical count-list projection heads into one helper: 302 -> 300.
    // r54 folds 3 package-obligation stock heads into one kind-parameterized renderer: 300 -> 298.
    expect(total).toBe(277);
    expect(total).toBeLessThan(299);
  });
});
