import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));
import { manufacturingCostAllocationProbe } from '../../../packages/server-manufacturing/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-manufacturing/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

const JOINED = 'SELECT 1 FROM manufacturing_cost_allocations allocation JOIN orders o ON o.id=allocation.order_id WHERE allocation.pool_id=pool.id';
const PLAIN = 'SELECT 1 FROM manufacturing_cost_allocations allocation WHERE allocation.pool_id=pool.id';

/** The cost-pool settlement fact built two cost-allocation probes with the
 * same SELECT head, differing only by the optional orders join. Originals were
 * captured verbatim from HEAD; one parameterized emitter now renders both byte
 * for byte on the caller's own statement. This suite pins that identity, the
 * single definition, the untouched lock/helper set and the lower read KPI. */
describe('manufacturing read-tail r28 cost-allocation probe family', () => {
  it('emits both former probes byte-identically through one parameterized emitter', () => {
    expect(manufacturingCostAllocationProbe(true)).toBe(JOINED);
    expect(manufacturingCostAllocationProbe(false)).toBe(PLAIN);
    expect(JOINED.startsWith(PLAIN.slice(0, PLAIN.indexOf(' WHERE')))).toBe(true);
    for (const sql of [JOINED, PLAIN]) {
      expect(sql).toContain('SELECT 1 FROM manufacturing_cost_allocations allocation');
      expect(sql).toContain('WHERE allocation.pool_id=pool.id');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('defines one text-only emitter and routes both probes through it', () => {
    const source = src('package-obligations.ts');
    expect(source.match(/export const manufacturingCostAllocationProbe =/g) ?? []).toHaveLength(1);
    expect(source.split('manufacturingCostAllocationProbe(').length - 1).toBe(2); // the true/false probes
    expect(source).not.toContain('EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation JOIN orders o ON o.id=allocation.order_id WHERE allocation.pool_id=pool.id)');
    expect(source).not.toContain('NOT EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation WHERE allocation.pool_id=pool.id)');
    const emitter = source.slice(source.indexOf('export const manufacturingCostAllocationProbe'), source.indexOf('export async function manufacturingPackageObligations'));
    expect(emitter.match(/SELECT/g) ?? []).toHaveLength(1);
    expect(emitter).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
  });

  it('leaves the r4/r11/r15-r27 helpers, locks and FOR UPDATE witness untouched', () => {
    expect(src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingExistsSelect');
    expect(src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingJsonbAggSql');
    expect(src('manufacturing-read-tail-r17-sql.ts')).toContain('export const manufacturingSettlementFactSelect');
    expect(src('finance-record-sources.ts')).toContain('const manufacturingJournalSelect=');
    expect(src('production-command-sources.ts')).toContain('WHERE id=? FOR UPDATE');
    expect(src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
  });

  it('keeps retirement:ts-statements strictly below the r27 BEFORE=58 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(53); // r27 left 58; r28 parameterizes the cost-allocation probe family 58->57; r29 scoped EXISTS probe 57->56; r30 scope CTE family 56->55; r31 allowed-select head family 55->53.
    expect(total).toBeLessThan(58);
  });
});
