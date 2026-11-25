import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  MANUFACTURING_RD_ORDER_SPECIFICATION_LINES,
  MANUFACTURING_RD_MATERIAL_RESERVATIONS,
  MANUFACTURING_RD_ORDER_POSTINGS,
  MANUFACTURING_RD_ORDER_OPERATIONS,
  MANUFACTURING_RD_LOT_GENEALOGY,
} from '../../../packages/server-manufacturing/src/adapters/native-manufacturing-read';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const registry = readFileSync(resolve(srcRoot, 'public/native_command.rs'), 'utf8');
const transport = readFileSync(resolve(srcRoot, 'adapters/native-manufacturing-read.ts'), 'utf8');

/** The five r22 detail selects, captured byte-for-byte before the cutover.
 * Their text now lives only in the frozen native owner registry; the TS caller
 * emits none of it and plans the matching closed kind on the caller's unit. */
const FAMILY: readonly {
  method: string;
  constant: string;
  kind: string;
  oracle: string;
  binds: string;
  scope: string;
}[] = [
  {
    method: 'orderSpecificationLines',
    constant: 'MANUFACTURING_RD_ORDER_SPECIFICATION_LINES',
    kind: MANUFACTURING_RD_ORDER_SPECIFICATION_LINES,
    oracle: 'SELECT l.*,p.tracking_mode FROM manufacturing_specification_lines l JOIN commerce_products p ON p.id=l.product_id WHERE l.specification_id=? ORDER BY l.ordinal',
    binds: '&[BindField::SpecificationId]',
    scope: '{specificationId:String(specification_id)}',
  },
  {
    method: 'orderReservations',
    constant: 'MANUFACTURING_RD_MATERIAL_RESERVATIONS',
    kind: MANUFACTURING_RD_MATERIAL_RESERVATIONS,
    oracle: 'SELECT r.*,p.name product_name,p.tracking_mode FROM manufacturing_material_reservations r JOIN commerce_products p ON p.id=r.product_id WHERE r.order_id=? ORDER BY p.name',
    binds: '&[BindField::OrderId]',
    scope: '{orderId:id}',
  },
  {
    method: 'orderPostings',
    constant: 'MANUFACTURING_RD_ORDER_POSTINGS',
    kind: MANUFACTURING_RD_ORDER_POSTINGS,
    oracle: 'SELECT p.*,j.status journal_status FROM manufacturing_order_postings p JOIN finance_journals j ON j.id=p.journal_id WHERE p.order_id=? ORDER BY p.source_order_version',
    binds: '&[BindField::OrderId]',
    scope: '{orderId:id}',
  },
  {
    method: 'orderOperations',
    constant: 'MANUFACTURING_RD_ORDER_OPERATIONS',
    kind: MANUFACTURING_RD_ORDER_OPERATIONS,
    oracle: 'SELECT op.*,r.name resource_name,e.labor_minutes,e.machine_minutes,e.evidence,e.created_at completed_at FROM manufacturing_specification_operations op JOIN production_resources r ON r.id=op.resource_id LEFT JOIN manufacturing_operation_events e ON e.operation_id=op.id AND e.order_id=? WHERE op.specification_id=? ORDER BY op.ordinal',
    binds: '&[BindField::OrderId, BindField::SpecificationId]',
    scope: '{orderId:id,specificationId:String(specification_id)}',
  },
  {
    method: 'lotGenealogy',
    constant: 'MANUFACTURING_RD_LOT_GENEALOGY',
    kind: MANUFACTURING_RD_LOT_GENEALOGY,
    oracle: 'SELECT g.*,i.lot_id input_lot_id,il.lot_code input_lot_code,il.serial_number input_serial_number,ol.lot_id output_lot_id,l.lot_code output_lot_code,l.serial_number output_serial_number FROM manufacturing_lot_genealogy g JOIN manufacturing_material_lot_issues i ON i.id=g.input_issue_id JOIN inventory_lots il ON il.id=i.lot_id JOIN manufacturing_output_lots ol ON ol.id=g.output_lot_id JOIN inventory_lots l ON l.id=ol.lot_id WHERE g.order_id=? ORDER BY g.id',
    binds: '&[BindField::OrderId]',
    scope: '{orderId:id}',
  },
];

describe('manufacturing read-tail r22 native detail-read cutover', () => {
  it('deletes the parameterized TS builder and routes all five callers to native kinds', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/manufacturing-read-tail-r22-sql.ts'))).toBe(false);
    const caller = src('production-command-sources.ts');
    expect(caller).not.toContain('manufacturingOrderDetailSelect');
    expect(caller).not.toContain('manufacturing-read-tail-r22-sql');
    expect(caller).toContain("from './native-manufacturing-read'");
    expect(caller).toContain('nativeManufacturingReadAll<Row>');
    for (const { method, constant, kind, scope, oracle } of FAMILY) {
      expect(caller).toContain(`async ${method}`);
      expect(caller).toContain(`nativeManufacturingReadAll<Row>(${constant},${scope},db)`);
      expect(caller).not.toContain(oracle);
      expect(kind).toMatch(/^[a-z-]+$/);
    }
    for (const needle of [
      'FROM manufacturing_specification_lines l ',
      'FROM manufacturing_material_reservations r ',
      'FROM manufacturing_order_postings p ',
      'FROM manufacturing_specification_operations op ',
      'FROM manufacturing_lot_genealogy g',
    ]) expect(caller).not.toContain(needle);
  });

  it('registers byte-identical owner plans with the caller bind order for all five kinds', () => {
    for (const { oracle, binds, kind } of FAMILY) {
      expect(registry).toContain(`"${oracle}"`);
      expect(registry).toContain(`kind: RD_${kind.replace(/-/g, '_').toUpperCase()}`);
      expect(registry).toContain(binds);
      expect(oracle).toContain('SELECT ');
      expect(oracle).toContain(' ORDER BY ');
      expect(oracle).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    expect(registry).toContain('assert_eq!(PLANS.len(), 13)');
    expect(registry).toContain('plan(RD_ORDER_SPECIFICATION_LINES).unwrap().sql');
  });

  it('admits all five kinds in the fail-closed native transport with no SQL of its own', () => {
    for (const { constant, kind } of FAMILY) {
      expect(transport).toContain(`export const ${constant} = '${kind}'`);
      expect(transport).toContain(`  ${constant},`);
    }
    expect(transport).not.toMatch(/FROM manufacturing_(order_postings|lot_genealogy|material_reservations)/);
    expect(transport).toContain("throw new ApiError(503, 'MANUFACTURING_READ_PLAN_INVALID'");
  });

  it('leaves the earlier helpers, locks and FOR UPDATE witness untouched', () => {
    const fragments = src('manufacturing-read-sql-fragments.ts');
    expect(fragments).toContain('export const manufacturingRowsSelect');
    expect(fragments).toContain('export const manufacturingIdSelect');
    expect(fragments).toContain('export const manufacturingAttachmentVersionGuard');
    for (const [name, symbol] of [
      ['manufacturing-read-tail-r15-sql.ts', 'manufacturingOrderJoinSelect'],
      ['manufacturing-read-tail-r16-sql.ts', 'manufacturingPurchaseRecordSelect'],
      ['manufacturing-read-tail-r17-sql.ts', 'manufacturingSettlementFactSelect'],
      ['manufacturing-read-tail-r18-sql.ts', 'manufacturingPurchaseRecordBranchSelect'],
      ['manufacturing-read-tail-r19-sql.ts', 'manufacturingAccountingProfileSelect'],
      ['manufacturing-read-tail-r20-sql.ts', 'manufacturingExistsGuard'],
      ['manufacturing-read-tail-r21-sql.ts', 'manufacturingAttachmentImageSelect'],
    ] as const) expect(src(name)).toContain(`export const ${symbol}`);
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
  });

  it('keeps retirement:ts-statements strictly below the r21 BEFORE=77 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(42); // r21 left 77; the r22-r37 folds reach 46; this cut retires the last r22 SELECT 46->45; package-obligation owner-table heads 45->42.
    expect(total).toBeLessThan(46);
    expect(total).toBeLessThan(77);
  });
});
