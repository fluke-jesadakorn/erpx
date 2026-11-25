import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { manufacturingOrderJoinSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r15-sql';
import { manufacturingStockFootprintSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r23-sql';

const adapter = (name: string) => new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url);

/** Native commerce output-product join, expanded exactly as the four r15 call sites pass it. */
const JP = 'JOIN commerce_products p ON p.id=s.output_product_id';

/** Byte oracles captured from the pre-fold (HEAD) r15 rendered text at each of
 * the four `production-command-sources.ts` callers. ORDERS and DETAILED are also
 * repeated verbatim in the native `native_command.rs` witnesses. */
const ORDERS = `SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${JP} WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC`;
const SCOPED = `SELECT o.*,s.output_product_id,s.batch_quantity_milli,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${JP} WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)`;
const DETAILED = `SELECT o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${JP} WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)`;
const OPEN = `SELECT o.*,s.name specification_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id WHERE o.company_id=? AND o.status NOT IN ('completed','cancelled') AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC`;

const FAMILY: readonly {
  name: string;
  args: readonly [string, string, string];
  oracle: string;
}[] = [
  {
    name: 'orders',
    args: ['o.*,s.name specification_name,p.name output_name', ` ${JP}`, 'o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC'],
    oracle: ORDERS,
  },
  {
    name: 'scopedOrder',
    args: ['o.*,s.output_product_id,s.batch_quantity_milli,p.tracking_mode output_tracking_mode', ` ${JP}`, 'o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)'],
    oracle: SCOPED,
  },
  {
    name: 'detailedOrder',
    args: ['o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode', ` ${JP}`, 'o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)'],
    oracle: DETAILED,
  },
  {
    name: 'openOrders',
    args: ['o.*,s.name specification_name', '', "o.company_id=? AND o.status NOT IN ('completed','cancelled') AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC"],
    oracle: OPEN,
  },
];

/** sha256 of the pre-fold HEAD `production-command-sources.ts`; pinning the
 * whole caller proves its four call sites and their binds are byte-unchanged. */
const CALLER_SHA256 = '3bbca63d700786746c0fa5c038c5e09d84c4e96b0416d8c86a17e039724024cf';

async function tsFiles(directory: URL): Promise<URL[]> {
  const found: URL[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory));
  }
  return found;
}

describe('manufacturing read-tail r34 r15/r23 head-fold dedupe', () => {
  it('renders all four r15 callers byte-identically to their pre-fold oracles', () => {
    for (const { args, oracle } of FAMILY) expect(manufacturingOrderJoinSelect(...args)).toBe(oracle);
    for (const { oracle } of FAMILY) {
      expect(oracle).toContain('SELECT ');
      expect(oracle).toContain(' FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id');
      expect(oracle).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps the native read witnesses byte-identical to the ORDERS/DETAILED oracles', async () => {
    const native = await readFile(new URL('../../../packages/server-manufacturing/src/public/native_command.rs', import.meta.url), 'utf8');
    expect(native).toContain(ORDERS);
    expect(native).toContain(DETAILED);
  });

  it('delegates the r15 head to the r23 footprint helper with no residual inline SQL', async () => {
    const r15 = await readFile(adapter('manufacturing-read-tail-r15-sql.ts'), 'utf8');
    const r23 = await readFile(adapter('manufacturing-read-tail-r23-sql.ts'), 'utf8');

    expect(r15.match(/export const manufacturingOrderJoinSelect/g) ?? []).toHaveLength(1);
    expect(r23.match(/export const manufacturingStockFootprintSelect/g) ?? []).toHaveLength(1);
    expect(r15).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    expect(r23).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);

    // The exact delegation arguments named by the scouted fold.
    expect(r15).toContain("from './manufacturing-read-tail-r23-sql'");
    expect(r15).toContain("manufacturingStockFootprintSelect(columns, 'manufacturing_specifications', 's', 's.id=o.specification_id')");
    expect(r15).not.toContain('FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id');

    // r23 now delegates the shared owner-table head to the r36 emitter while
    // owning the single footprint join tail.
    expect(r23).toContain("from './manufacturing-read-tail-r36-sql'");
    expect(r23).toContain('manufacturingOwnerTableSelect(columns, `orders o JOIN ${table} ${alias} ON ${on}`)');
    expect(r23).not.toContain('`SELECT ${columns} FROM manufacturing_orders');
    expect(manufacturingStockFootprintSelect('x', 'manufacturing_specifications', 's', 's.id=o.specification_id'))
      .toBe('SELECT x FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id');
  });

  it('leaves the four callers and their binds byte-unchanged', async () => {
    const caller = await readFile(adapter('production-command-sources.ts'), 'utf8');
    expect(caller.split('manufacturingOrderJoinSelect(').length - 1).toBe(4);
    expect(createHash('sha256').update(caller).digest('hex')).toBe(CALLER_SHA256);
  });

  it('leaves locks/native/cross-owner witnesses untouched', async () => {
    const caller = await readFile(adapter('production-command-sources.ts'), 'utf8');
    expect(caller).toContain('${manufacturingIdSelect("manufacturing_orders")} WHERE id=? FOR UPDATE');
    const locks = await readFile(adapter('finance-record-sources.ts'), 'utf8');
    expect(locks).toContain('LOCK TABLE planning_scenarios,planning_recommendations');
    expect(locks).toContain('LOCK TABLE manufacturing_specifications,manufacturing_specification_lines');
    for (const name of ['manufacturing-read-tail-r15-sql.ts', 'manufacturing-read-tail-r23-sql.ts']) {
      expect(await readFile(adapter(name), 'utf8')).not.toMatch(/LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE/i);
    }
  });

  it('pins the manufacturing owner SELECT KPI at 45 after the r22 native detail-read cut (46->45)', async () => {
    let total = 0;
    for (const file of await tsFiles(new URL('../../../packages/server-manufacturing/src/', import.meta.url))) {
      total += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    }
    expect(total).toBe(45);
    expect(total).toBeLessThan(50);
  });
});
