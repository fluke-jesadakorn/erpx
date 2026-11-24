import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { manufacturingPurchaseRecordBranchSelect } from '../../../packages/server-manufacturing/src/adapters/manufacturing-read-tail-r18-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-manufacturing/src/adapters/${name}`, import.meta.url), 'utf8');

/** The four pre-dedupe UNION branch literals, captured verbatim from the source
 * before the helper replaced them. Sources are the exact caller expressions so
 * byte identity is checked against the originals rather than a matching shape. */
const BRANCHES: readonly {
  args: readonly [string, string, string, string, string, string, string];
  original: string;
}[] = [
  {
    args: ["'specification'::text kind", "'manufacturing_spec'::text record_type", "'manufacturing'::text module_id", "'manufacturing.bom'::text capability", "witness", "manufacturingPurchaseSpecificationRows", "spec"],
    original: "SELECT 'specification'::text kind,'manufacturing_spec'::text record_type,'manufacturing'::text module_id,'manufacturing.bom'::text capability,id,company_id,branch_id,advanced,witness FROM (manufacturingPurchaseSpecificationRows) spec",
  },
  {
    args: ["'order'", "'manufacturing_order'", "'manufacturing'", "'manufacturing.execution'", "witness", "manufacturingPurchaseOrderRows", "production"],
    original: "SELECT 'order','manufacturing_order','manufacturing','manufacturing.execution',id,company_id,branch_id,advanced,witness FROM (manufacturingPurchaseOrderRows) production",
  },
  {
    args: ["'scenario'", "'planning_scenario'", "'planning'", "'planning.basic'", "source_witness", "planningRows(planningPurchaseScenarioRows)", "scenario"],
    original: "SELECT 'scenario','planning_scenario','planning','planning.basic',id,company_id,branch_id,advanced,source_witness FROM (planningRows(planningPurchaseScenarioRows)) scenario",
  },
  {
    args: ["'recommendation'", "'planning_recommendation'", "'planning'", "'planning.basic'", "source_witness", "planningRows(planningPurchaseRecommendationRows)", "recommendation"],
    original: "SELECT 'recommendation','planning_recommendation','planning','planning.basic',id,company_id,branch_id,advanced,source_witness FROM (planningRows(planningPurchaseRecommendationRows)) recommendation",
  },
];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('manufacturing read-tail r18 parameterized family dedupe', () => {
  it('renders the shared purchase-record branch select byte-identically to all four originals', () => {
    for (const { args, original } of BRANCHES) {
      expect(manufacturingPurchaseRecordBranchSelect(...args)).toBe(original);
    }
  });

  it('routes every family call site through the shared helper with no residual inline SQL', async () => {
    const target = await src('purchases.ts');
    expect(target.split('from "./manufacturing-read-tail-r18-sql"').length - 1).toBe(1);
    expect(target.split('manufacturingPurchaseRecordBranchSelect(').length - 1).toBe(4);
    for (const { original } of BRANCHES) expect(target).not.toContain(original);
    expect(target).not.toContain("UNION ALL SELECT 'order'");
    expect(target).not.toContain("UNION ALL SELECT 'recommendation'");
  });

  it('keeps the helper read-only and leaves r11/r4/r15/r16/r17 helpers and locks untouched', async () => {
    for (const { original } of BRANCHES) {
      expect(original).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const helper = await src('manufacturing-read-tail-r18-sql.ts');
    expect(helper).toContain('export const manufacturingPurchaseRecordBranchSelect');
    expect(helper).not.toMatch(/prepare\(|\.run\(|\.all\(|\.first\(/);
    expect(await src('attachment-read-sources.ts')).toContain('manufacturingOrderLineageAgg');
    expect(await src('finance-record-sources.ts')).toContain('manufacturingJournalLineSum');
    const command = await src('production-command-sources.ts');
    expect(command).toContain('approvedSpecGuardSql');
    expect(command).toContain('operationEventDoneGuardSql');
    expect(command).toContain('manufacturingOrderJoinSelect(');
    expect(await src('manufacturing-read-tail-r15-sql.ts')).toContain('export const manufacturingOrderJoinSelect');
    expect(await src('manufacturing-read-tail-r16-sql.ts')).toContain('export const manufacturingPurchaseRecordSelect');
    expect(await src('manufacturing-read-tail-r17-sql.ts')).toContain('export const manufacturingSettlementFactSelect');
    expect(await src('manufacturing-read-sql-fragments.ts')).toContain('export const manufacturingRowsSelect');
    expect(await src('package-obligations.ts')).toContain('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy');
    expect(await src('purchases.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events');
    expect(command).toContain('WHERE id=? FOR UPDATE');
  });

  it('lowers the manufacturing owner SELECT KPI strictly below the recorded BEFORE=89 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-manufacturing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(89);
    expect(count).toBeLessThanOrEqual(86); // r17 lowered 94->89; r18 lowers 89->86.
  });
});
