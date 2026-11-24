import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { manufacturingSettlementFactSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r17-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url), 'utf8');

/** The six pre-dedupe UNION branch literals, captured verbatim from the source
 * before the helper replaced them. The helper wraps text, so byte identity is
 * checked against the exact originals rather than a matching template. */
const BRANCHES: readonly (readonly [string, string, string])[] = [
  [
    "'order' kind,o.id,to_jsonb(o)::text fact",
    "orders o WHERE o.status NOT IN ('completed','cancelled') OR NOT o.source_valid",
    "SELECT 'order' kind,o.id,to_jsonb(o)::text fact FROM orders o WHERE o.status NOT IN ('completed','cancelled') OR NOT o.source_valid",
  ],
  [
    "'specification',s.id,to_jsonb(s)::text",
    "manufacturing_specifications s,scope WHERE scope.full AND s.company_id=scope.company_id AND s.status NOT IN ('approved','rejected','retired')",
    "SELECT 'specification',s.id,to_jsonb(s)::text FROM manufacturing_specifications s,scope WHERE scope.full AND s.company_id=scope.company_id AND s.status NOT IN ('approved','rejected','retired')",
  ],
  [
    "'accounting-profile',p.id,to_jsonb(p)::text",
    "manufacturing_accounting_profiles p,scope WHERE scope.full AND p.company_id=scope.company_id AND p.status NOT IN ('approved','rejected','retired')",
    "SELECT 'accounting-profile',p.id,to_jsonb(p)::text FROM manufacturing_accounting_profiles p,scope WHERE scope.full AND p.company_id=scope.company_id AND p.status NOT IN ('approved','rejected','retired')",
  ],
  [
    "'material-reservation',r.id,to_jsonb(r)::text",
    "manufacturing_material_reservations r JOIN orders o ON o.id=r.order_id WHERE r.quantity_milli>r.consumed_milli+r.released_milli",
    "SELECT 'material-reservation',r.id,to_jsonb(r)::text FROM manufacturing_material_reservations r JOIN orders o ON o.id=r.order_id WHERE r.quantity_milli>r.consumed_milli+r.released_milli",
  ],
  [
    "'cost-pool',pool.id,to_jsonb(pool)::text",
    "manufacturing_cost_pools pool,scope WHERE pool.company_id=scope.company_id AND pool.status NOT IN ('allocated','reversed') AND (scope.full OR EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation JOIN orders o ON o.id=allocation.order_id WHERE allocation.pool_id=pool.id) OR NOT EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation WHERE allocation.pool_id=pool.id))",
    "SELECT 'cost-pool',pool.id,to_jsonb(pool)::text FROM manufacturing_cost_pools pool,scope WHERE pool.company_id=scope.company_id AND pool.status NOT IN ('allocated','reversed') AND (scope.full OR EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation JOIN orders o ON o.id=allocation.order_id WHERE allocation.pool_id=pool.id) OR NOT EXISTS(SELECT 1 FROM manufacturing_cost_allocations allocation WHERE allocation.pool_id=pool.id))",
  ],
  [
    "'completed-cost',o.id,to_jsonb(o)::text",
    "orders o LEFT JOIN manufacturing_cost_snapshots cost ON cost.order_id=o.id WHERE o.status='completed' AND (cost.id IS NULL OR cost.total_minor<>o.material_cost_minor+o.labor_cost_minor+o.overhead_cost_minor OR cost.output_quantity_milli IS DISTINCT FROM o.reported_output_milli)",
    "SELECT 'completed-cost',o.id,to_jsonb(o)::text FROM orders o LEFT JOIN manufacturing_cost_snapshots cost ON cost.order_id=o.id WHERE o.status='completed' AND (cost.id IS NULL OR cost.total_minor<>o.material_cost_minor+o.labor_cost_minor+o.overhead_cost_minor OR cost.output_quantity_milli IS DISTINCT FROM o.reported_output_milli)",
  ],
];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('manufacturing read-tail r17 parameterized family dedupe', () => {
  it('renders the shared settlement-fact select byte-identically to all six originals', () => {
    for (const [projection, source, original] of BRANCHES) {
      expect(manufacturingSettlementFactSelect(projection, source)).toBe(original);
    }
  });

  it('routes every family call site through the shared helper with no residual inline SQL', async () => {
    const target = await src('package-obligations.ts');
    expect(target.split('from "./manufacturing-read-tail-r17-sql"').length - 1).toBe(1);
    expect(target.split('manufacturingSettlementFactSelect(').length - 1).toBe(6);
    for (const [, , original] of BRANCHES) expect(target).not.toContain(original);
    expect(target).not.toContain("UNION ALL SELECT 'specification'");
    expect(target).not.toContain("UNION ALL SELECT 'completed-cost'");
  });

  it('keeps the helper read-only and leaves r11/r4/r15/r16 helpers and locks untouched', async () => {
    for (const [, , original] of BRANCHES) {
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const helper = await src('manufacturing-read-tail-r17-sql.ts');
    expect(helper).toContain('export const manufacturingSettlementFactSelect');
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    expect(await src('attachment-read-sources.ts')).toContain('manufacturingOrderLineageAgg');
    expect(await src('finance-record-sources.ts')).toContain('manufacturingJournalLineSum');
    const command = await src('production-command-sources.ts');
    expect(command).toContain('approvedSpecGuardSql');
    expect(command).toContain('operationEventDoneGuardSql');
    expect(command).toContain('manufacturingOrderJoinSelect(');
    expect(await src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(await src('manufacturing-read-tail-r16-sql.ts')).toContain('export const manufacturingPurchaseRecordSelect');
    expect(await src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingRowsSelect');
    expect(await src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(await src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(command).toContain('WHERE id=? FOR UPDATE');
  });

  it('lowers the manufacturing owner SELECT KPI strictly below the recorded BEFORE=94 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-manufacturing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(94);
    expect(count).toBe(89); // r16 lowered 96->94; r17 lowers 94->89.
  });
});
