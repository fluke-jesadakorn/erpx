import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { manufacturingOrderJoinSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r15-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url), 'utf8');

/** Native commerce output-product join, expanded exactly as the call sites pass it. */
const JP = 'JOIN commerce_products p ON p.id=s.output_product_id';

/** Byte-identical originals, captured from the pre-dedupe source with the
 * native projection expanded. */
const ORDERS = `SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${JP} WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC`;
const SCOPED = `SELECT o.*,s.output_product_id,s.batch_quantity_milli,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${JP} WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)`;
const DETAILED = `SELECT o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id ${JP} WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)`;
const OPEN = `SELECT o.*,s.name specification_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id WHERE o.company_id=? AND o.status NOT IN ('completed','cancelled') AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('manufacturing read-tail r15 parameterized family dedupe', () => {
  it('renders the shared order-join family byte-identically to all four originals', () => {
    expect(manufacturingOrderJoinSelect('o.*,s.name specification_name,p.name output_name', ` ${JP}`, 'o.company_id=? AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC')).toBe(ORDERS);
    expect(manufacturingOrderJoinSelect('o.*,s.output_product_id,s.batch_quantity_milli,p.tracking_mode output_tracking_mode', ` ${JP}`, 'o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)')).toBe(SCOPED);
    expect(manufacturingOrderJoinSelect('o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode', ` ${JP}`, 'o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?)')).toBe(DETAILED);
    expect(manufacturingOrderJoinSelect('o.*,s.name specification_name', '', "o.company_id=? AND o.status NOT IN ('completed','cancelled') AND (?::text IS NULL OR o.branch_id=?) ORDER BY o.updated_at DESC")).toBe(OPEN);
  });

  it('routes every family call site through the shared helper with no residual inline SQL', async () => {
    const target = await src('production-command-sources.ts');
    expect(target.split("from './manufacturing-read-tail-r15-sql'").length - 1).toBe(1);
    expect(target.split('manufacturingOrderJoinSelect(').length - 1).toBe(4); // four call sites
    for (const inline of [
      '`SELECT o.*,s.name specification_name,p.name output_name FROM manufacturing_orders o JOIN',
      '`SELECT o.*,s.output_product_id,s.batch_quantity_milli,p.tracking_mode output_tracking_mode FROM manufacturing_orders',
      '`SELECT o.*,s.name specification_name,s.mode,p.name output_name,p.tracking_mode output_tracking_mode FROM manufacturing_orders',
      '"SELECT o.*,s.name specification_name FROM manufacturing_orders o JOIN manufacturing_specifications s ON s.id=o.specification_id WHERE',
    ]) expect(target).not.toContain(inline);
  });

  it('keeps the helper read-only and leaves locks/advisory reads untouched', async () => {
    for (const fragment of [ORDERS, SCOPED, DETAILED, OPEN]) {
      expect(fragment).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const target = await src('production-command-sources.ts');
    expect(target).toContain('${manufacturingIdSelect("manufacturing_orders")} WHERE id=? FOR UPDATE');
    const locks = await src('finance-record-sources.ts');
    expect(locks).toContain('LOCK TABLE planning_scenarios,planning_recommendations');
    expect(locks).toContain('LOCK TABLE manufacturing_specifications,manufacturing_specification_lines');
  });

  it('lowers the manufacturing owner SELECT KPI strictly below the recorded BEFORE=99 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-manufacturing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(99);
    expect(count).toBe(94); // r16 lowered it from 96; helper bytes unchanged.
  });
});
