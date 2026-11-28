import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
/** Pre-r26 byte oracle for the retired builder; no longer a live query source. */
const planningSelectFrom = (columns: string, indent: string, from: string) =>
  `SELECT ${columns}\n${indent}FROM ${from}`;
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe declaration from the r18 BEFORE=20 source. The capacity
// operations reader and the two executive projections share one parameterized
// projection/FROM head: only the fixed columns, indent and FROM fragment differ.
// These are byte-identity oracles only, never live query sources.
const ORIG_DECL = "      operations: async (specificationId, companyId) => (await db.prepare(`SELECT o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id\n        ${productionmanufacturingProjection.routingOperations} JOIN production_resources r ON r.id=o.resource_id\n        WHERE o.specification_id=? AND r.company_id=? ORDER BY o.ordinal`).bind(specificationId, companyId).all<CapacityOperation>()).results,";
const NEW_DECL = "      operations: async (specificationId, companyId) => (await db.prepare(planningSelectFrom('o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id',\n        '        ', `${productionmanufacturingProjection.routingOperations.replace(/^FROM /, '')} JOIN production_resources r ON r.id=o.resource_id\n        WHERE o.specification_id=? AND r.company_id=? ORDER BY o.ordinal`)).bind(specificationId, companyId).all<CapacityOperation>()).results,";
const ORIG_SQL = "SELECT o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id\n        FROM manufacturing_specification_operations o JOIN production_resources r ON r.id=o.resource_id\n        WHERE o.specification_id=? AND r.company_id=? ORDER BY o.ordinal";
const routingOperations = 'FROM manufacturing_specification_operations o';
const generated = planningSelectFrom('o.*,r.version resource_version,r.status,r.daily_minutes,r.work_weekdays_json,r.branch_id',
  '        ', `${routingOperations.replace(/^FROM /, '')} JOIN production_resources r ON r.id=o.resource_id
        WHERE o.specification_id=? AND r.company_id=? ORDER BY o.ordinal`);

describe('Planning read-tail r19 capacity projection/FROM head', () => {
  test('head reproduces the former inline capacity statement byte-identically', () => {
    expect(generated).toBe(ORIG_SQL);
    const projectionSrc = readFileSync(resolve(srcRoot, '../../server-manufacturing/src/adapters/production-read-projections.ts'), 'utf8');
    expect(projectionSrc).toContain('"routingOperations": "FROM manufacturing_specification_operations o"');
  });

  test('capacity operations now compose the surviving single-line builder', () => {
    const current = src('capacity.ts');
    expect(current).toContain('planningPackageFactRows(');
    expect(current).not.toContain(NEW_DECL);
    expect(current).not.toContain(ORIG_DECL);
    expect(current).not.toContain('planningSelectFrom(');
  });

  test('the retired shared builder is deleted; its byte oracle stays read-only', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-from-sql.ts'))).toBe(false);
    expect(selectCount(generated)).toBe(1);
    expect(generated.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|PG_ADVISORY/i);
  });

  test('r1-r18 helpers, native reads and all lock statements stay untouched', () => {
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('planning-scoped-list-sql.ts')).toContain('planningScopedBranchList');
    expect(existsSync(resolve(srcRoot, 'adapters/planning-scoped-row-sql.ts'))).toBe(false);
    expect(src('planning-calendar-source-sql.ts')).toContain('resourceCalendarByKey');
    expect(src('planning-resource-allocation-sql.ts')).toContain('planningResourceAllocationRows');
    expect(src('planning-jsonb-row-sql.ts')).toContain('planningJsonbRowByKey');
    expect(src('planning-jsonb-agg-sql.ts')).toContain('planningJsonbAggOrdered');
    expect(src('planning-package-fact-sql.ts')).toContain('planningPackageFactRows');
    expect(src('planning-image-source-sql.ts')).toContain('planningSourceRows');
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-from-sql.ts'))).toBe(false);
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('package-obligations.ts')).toContain('planningPurchaseSourceReadLocks(database)');
    const capacity = src('capacity.ts');
    expect(capacity).toContain('planningPackageFactRows(');
    expect(capacity).toContain('resourceCalendarByKey(');
    expect(capacity).toContain('planningResourceAllocationRows(');
    expect(capacity).not.toContain('LOCK TABLE');
    const read = src('production-command-sources-read.ts');
    expect(read).toContain('nativeExecutiveShortage(');
    expect(read).toContain('nativeExecutiveRecommendation(');
    expect(read).toContain('nativeComparison(');
    expect(read).not.toContain('LOCK TABLE');
    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningSelectFrom');
        expect(text).not.toContain('planningSelectColumnsFrom');
      }
    }
  });

  test('retirement:ts-statements stays strictly below the r18 BEFORE=20 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r19 folds the capacity operations projection/FROM head into the r17
    // builder (20 -> 19) while keeping the emitted SQL byte-identical; r20
    // folds the production-resources guard into the r7 EXISTS builder (19 -> 18).
    // r22 folded the resource-allocation point lookup into the package fact builder (17 -> 16).
    // r23 folded the resource-calendar point lookup into the single-line projection/FROM builder (16 -> 15).
    // r24 folded the scoped single-row family into that same builder (15 -> 14).
    // r25 folded the scoped branch-list family into the single-line projection/FROM builder (14 -> 13).
    expect(total).toBe(5);
    expect(total).toBeLessThan(19);
  });
});
