import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { manufacturingOrderDetailSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r22-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

/** Owner-supplied projection fragments are placeholders here: the helper only
 * concatenates its four arguments, so whole-string byte identity of the
 * surrounding SQL is the oracle. Originals were captured verbatim from HEAD.
 * `${...}` fragments are literal text and multi-line join tails keep their
 * newline plus six-space indent, exactly as the five former inline literals. */
const MP = '${productioncommerceProjection.materialProduct}';
const RP = '${productioncommerceProjection.reservationProduct}';
const PJ = '${productionfinanceProjection.journal}';
const OR = '${productionplanningProjection.operationResource}';
const GI = '${productioninventoryProjection.genealogyInputLot}';
const GO = '${productioninventoryProjection.genealogyOutputLot}';

const FAMILY: readonly {
  method: string;
  args: readonly [string, string, string, string];
  original: string;
}[] = [
  {
    method: 'orderSpecificationLines',
    args: [
      'l.*,p.tracking_mode',
      ` FROM manufacturing_specification_lines l ${MP}`,
      'l.specification_id=?',
      'l.ordinal',
    ],
    original: `SELECT l.*,p.tracking_mode FROM manufacturing_specification_lines l ${MP} WHERE l.specification_id=? ORDER BY l.ordinal`,
  },
  {
    method: 'orderReservations',
    args: [
      'r.*,p.name product_name,p.tracking_mode',
      ` FROM manufacturing_material_reservations r ${RP}`,
      'r.order_id=?',
      'p.name',
    ],
    original: `SELECT r.*,p.name product_name,p.tracking_mode FROM manufacturing_material_reservations r ${RP} WHERE r.order_id=? ORDER BY p.name`,
  },
  {
    method: 'orderPostings',
    args: [
      'p.*,j.status journal_status',
      ` FROM manufacturing_order_postings p ${PJ}`,
      'p.order_id=?',
      'p.source_order_version',
    ],
    original: `SELECT p.*,j.status journal_status FROM manufacturing_order_postings p ${PJ} WHERE p.order_id=? ORDER BY p.source_order_version`,
  },
  {
    method: 'orderOperations',
    args: [
      `op.*,r.name resource_name,e.labor_minutes,e.machine_minutes,e.evidence,e.created_at completed_at`,
      `\n      FROM manufacturing_specification_operations op ${OR}\n      LEFT JOIN manufacturing_operation_events e ON e.operation_id=op.id AND e.order_id=?`,
      'op.specification_id=?',
      'op.ordinal',
    ],
    original: `SELECT op.*,r.name resource_name,e.labor_minutes,e.machine_minutes,e.evidence,e.created_at completed_at\n      FROM manufacturing_specification_operations op ${OR}\n      LEFT JOIN manufacturing_operation_events e ON e.operation_id=op.id AND e.order_id=? WHERE op.specification_id=? ORDER BY op.ordinal`,
  },
  {
    method: 'lotGenealogy',
    args: [
      `g.*,i.lot_id input_lot_id,il.lot_code input_lot_code,il.serial_number input_serial_number,\n      ol.lot_id output_lot_id,l.lot_code output_lot_code,l.serial_number output_serial_number`,
      ` FROM manufacturing_lot_genealogy g\n      JOIN manufacturing_material_lot_issues i ON i.id=g.input_issue_id ${GI}\n      JOIN manufacturing_output_lots ol ON ol.id=g.output_lot_id ${GO}`,
      'g.order_id=?',
      'g.id',
    ],
    original: `SELECT g.*,i.lot_id input_lot_id,il.lot_code input_lot_code,il.serial_number input_serial_number,\n      ol.lot_id output_lot_id,l.lot_code output_lot_code,l.serial_number output_serial_number FROM manufacturing_lot_genealogy g\n      JOIN manufacturing_material_lot_issues i ON i.id=g.input_issue_id ${GI}\n      JOIN manufacturing_output_lots ol ON ol.id=g.output_lot_id ${GO} WHERE g.order_id=? ORDER BY g.id`,
  },
];

describe('manufacturing read-tail r22 production detail select family', () => {
  it('renders the shared detail select byte-identically to all five originals and stays read-only', () => {
    for (const { args, original } of FAMILY) {
      expect(manufacturingOrderDetailSelect(...args)).toBe(original);
      expect(original).toContain('SELECT ');
      expect(original).toContain(' ORDER BY ');
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only helper, routes all five callers and leaves no residual inline SQL', () => {
    const helper = src('manufacturing-read-tail-r22-sql.ts');
    expect(helper.match(/export const manufacturingOrderDetailSelect/g) ?? []).toHaveLength(1);
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    const caller = src('production-command-sources.ts');
    expect(caller.split('manufacturingOrderDetailSelect(').length - 1).toBe(5);
    expect(caller).toContain("from './manufacturing-read-tail-r22-sql'");
    for (const { method, args, original } of FAMILY) {
      expect(caller).toContain(`async ${method}`);
      for (const arg of args) expect(caller).toContain(arg);
      expect(caller).not.toContain(original);
    }
    for (const needle of [
      'SELECT l.*,p.tracking_mode FROM',
      'SELECT r.*,p.name product_name FROM',
      'SELECT p.*,j.status journal_status FROM',
      'SELECT op.*,r.name resource_name FROM',
      'SELECT g.*,i.lot_id input_lot_id FROM',
    ]) expect(caller).not.toContain(needle);
  });

  it('leaves the r4/r11/r15-r21 helpers, locks and FOR UPDATE witness untouched', () => {
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
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(src('finance-record-sources.ts')).toContain('LOCK TABLE planning_scenarios');
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('manufacturing-read-tail-r22-sql.ts')).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE/i);
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
    expect(total).toBeLessThanOrEqual(72); // r21 left 77; r22 lowers 77->73 (five detail selects -> one helper); r23 lowers 73->72.
    expect(total).toBeLessThan(77);
  });
});
