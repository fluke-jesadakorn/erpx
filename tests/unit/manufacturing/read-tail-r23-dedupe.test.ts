import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { manufacturingStockFootprintSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r23-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** The two stock-footprint UNION branches were byte-identical apart from their
 * projected columns, joined table/alias and join predicate. Originals were
 * captured verbatim from HEAD; the helper concatenates its four arguments, so
 * whole-string byte identity of each emitted branch is the oracle. */
const FAMILY: readonly {
  args: readonly [string, string, string, string];
  original: string;
}[] = [
  {
    args: [
      'o.id order_id,o.company_id,o.warehouse_id,s.output_product_id product_id',
      'manufacturing_specifications',
      's',
      's.id=o.specification_id AND s.company_id=o.company_id',
    ],
    original: `SELECT o.id order_id,o.company_id,o.warehouse_id,s.output_product_id product_id FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id AND s.company_id=o.company_id`,
  },
  {
    args: [
      'o.id,o.company_id,o.warehouse_id,line.product_id',
      'manufacturing_specification_lines',
      'line',
      'line.specification_id=o.specification_id',
    ],
    original: `SELECT o.id,o.company_id,o.warehouse_id,line.product_id FROM manufacturing_orders o JOIN manufacturing_specification_lines line ON line.specification_id=o.specification_id`,
  },
];

describe('manufacturing read-tail r23 stock-footprint select family', () => {
  it('renders the shared stock-footprint select byte-identically to both originals and stays read-only', () => {
    for (const { args, original } of FAMILY) {
      expect(manufacturingStockFootprintSelect(...args)).toBe(original);
      expect(original).toContain('SELECT ');
      expect(original).toContain(' JOIN ');
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only helper, routes both callers and leaves no residual inline SQL', () => {
    const helper = src('manufacturing-read-tail-r23-sql.ts');
    expect(helper.match(/export const manufacturingStockFootprintSelect/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const caller = src('package-obligations.ts');
    expect(caller.split('manufacturingStockFootprintSelect(').length - 1).toBe(2);
    expect(caller).toContain('from "./manufacturing-read-tail-r23-sql"');
    for (const { original } of FAMILY) expect(caller).not.toContain(original);
    expect(caller).not.toContain('SELECT o.id order_id,o.company_id,o.warehouse_id,s.output_product_id');
    expect(caller).not.toContain('SELECT o.id,o.company_id,o.warehouse_id,line.product_id');
  });

  it('leaves the r4/r11/r15-r22 helpers, locks and FOR UPDATE witness untouched', () => {
    const fragments = src('manufacturing-read-sql-fragments.ts');
    expect(fragments).toContain('export const manufacturingRowsSelect');
    expect(fragments).toContain('export const manufacturingIdSelect');
    expect(fragments).toContain('export const manufacturingAttachmentVersionGuard');
    expect(src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(src('manufacturing-read-tail-r16-sql.ts')).toContain('export const manufacturingPurchaseRecordSelect');
    expect(src('manufacturing-read-tail-r17-sql.ts')).toContain('export const manufacturingSettlementFactSelect');
    expect(src('manufacturing-read-tail-r18-sql.ts')).toContain('export const manufacturingPurchaseRecordBranchSelect');
    expect(src('manufacturing-read-tail-r19-sql.ts')).toContain('export const manufacturingAccountingProfileSelect');
    expect(src('manufacturing-read-tail-r20-sql.ts')).toContain('export const manufacturingExistsGuard');
    expect(src('manufacturing-read-tail-r21-sql.ts')).toContain('export const manufacturingAttachmentImageSelect');
    expect(src('manufacturing-read-tail-r22-sql.ts')).toContain('export const manufacturingOrderDetailSelect');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(src('finance-record-sources.ts')).toContain('LOCK TABLE planning_scenarios');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('manufacturing-read-tail-r23-sql.ts')).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  it('keeps retirement:ts-statements strictly below the r23 BEFORE=72 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(60); // r23 left 72; r24 72->70; r25 70->65; r26 unifies six EXISTS guards -> 60.
    expect(total).toBeLessThan(65);
  });
});
