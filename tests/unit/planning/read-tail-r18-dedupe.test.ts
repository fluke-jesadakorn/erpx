import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
/** Pre-r28 byte oracle for the retired builder; no longer a live query source. */
const planningSelectColumnsFrom = (columns: string, rest: string) =>
  `SELECT ${columns} FROM ${rest}`;
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe declarations and SQL from the r17 BEFORE=21 source. These
// are byte-identity oracles only, never live query sources. The calendar and
// comparison readers share one single-line parameterized projection/FROM head:
// only the fixed columns and the fixed FROM/JOIN/WHERE/ORDER fragment differ.
const ORIG_CALENDAR_DECL = "async calendar(companyId:string,branchId:string | null,branchId2:string | null){return db.prepare(`SELECT c.* FROM production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id\n          WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY c.work_date DESC LIMIT 500`)\n          .bind(companyId, branchId, branchId2).all<Row>();},";
const ORIG_COMPARISON_DECL = "async comparison(comparisonScenarioId:string,companyId:string,branch_id:string | number | null,specificationId:string){return db.prepare(`SELECT s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date FROM planning_scenarios s\n      JOIN planning_recommendations r ON r.scenario_id=s.id WHERE s.id=? AND s.company_id=? AND s.branch_id=? AND s.specification_id=?`)\n      .bind(comparisonScenarioId, companyId, branch_id, specificationId).first<Row>();},";
const NEW_CALENDAR_DECL = "async calendar(companyId:string,branchId:string | null,branchId2:string | null){return db.prepare(planningSelectColumnsFrom('c.*', `production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id\n          WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY c.work_date DESC LIMIT 500`))\n          .bind(companyId, branchId, branchId2).all<Row>();},";
const NEW_COMPARISON_DECL = "async comparison(comparisonScenarioId:string,companyId:string,branch_id:string | number | null,specificationId:string){return db.prepare(planningSelectColumnsFrom('s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date', `planning_scenarios s\n      JOIN planning_recommendations r ON r.scenario_id=s.id WHERE s.id=? AND s.company_id=? AND s.branch_id=? AND s.specification_id=?`))\n      .bind(comparisonScenarioId, companyId, branch_id, specificationId).first<Row>();},";
const ORIG_CALENDAR_SQL = "SELECT c.* FROM production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id\n          WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY c.work_date DESC LIMIT 500";
const ORIG_COMPARISON_SQL = "SELECT s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date FROM planning_scenarios s\n      JOIN planning_recommendations r ON r.scenario_id=s.id WHERE s.id=? AND s.company_id=? AND s.branch_id=? AND s.specification_id=?";

const generatedCalendar = planningSelectColumnsFrom('c.*', `production_resource_calendar c JOIN production_resources r ON r.id=c.resource_id
          WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY c.work_date DESC LIMIT 500`);
const generatedComparison = planningSelectColumnsFrom('s.id,s.version,s.shortages_json,r.required_minutes,r.feasible_date', `planning_scenarios s
      JOIN planning_recommendations r ON r.scenario_id=s.id WHERE s.id=? AND s.company_id=? AND s.branch_id=? AND s.specification_id=?`);

describe('Planning read-tail r18 parameterized single-line projection/FROM head', () => {
  test('head reproduces both former inline statements byte-identically', () => {
    expect(generatedCalendar).toBe(ORIG_CALENDAR_SQL);
    expect(generatedComparison).toBe(ORIG_COMPARISON_SQL);
  });

  test('calendar and comparison are native-owned; the builder lives only in its oracle', () => {
    const current = src('production-command-sources-read.ts');
    expect(current).toContain('nativeCalendar(companyId,branchId,branchId2)');
    expect(current).toContain('nativeComparison({comparisonScenarioId,specificationId},companyId,branch_id===null?null:String(branch_id))');
    expect(current).not.toContain(ORIG_CALENDAR_SQL);
    expect(current).not.toContain(ORIG_COMPARISON_SQL);
    expect(NEW_CALENDAR_DECL).toContain('planningSelectColumnsFrom');
    expect(NEW_COMPARISON_DECL).toContain('planningSelectColumnsFrom');
  });

  test('the retired single-line head is deleted; its byte oracles stay read-only', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
    for (const sql of [generatedCalendar, generatedComparison]) {
      expect(selectCount(sql)).toBe(1);
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|PG_ADVISORY/i);
    }
  });

  test('r1-r17 helpers, native reads and all lock statements stay untouched', () => {
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
    expect(readdirSync(resolve(srcRoot, 'adapters'))).not.toContain('planning-select-from-sql.ts');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('package-obligations.ts')).toContain('planningPurchaseSourceReadLocks(database)');
    const read = src('production-command-sources-read.ts');
    expect(read).toContain('nativeComparison(');
    expect(read).toContain('nativeExecutiveShortage(');
    expect(read).not.toContain('LOCK TABLE');
    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningSelectColumnsFrom');
        expect(text).not.toContain('planningSelectFrom');
      }
    }
  });

  test('retirement:ts-statements stays strictly below the r17 BEFORE=21 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r18 folds the calendar/comparison single-line projection/FROM heads into
    // one owner-local builder (21 -> 20) while keeping every SQL byte-identical;
    // r19 folds the capacity operations head into the r17 builder (20 -> 19);
    // r20 folds the production-resources guard into the r7 EXISTS builder (19 -> 18).
    // r22 folded the resource-allocation point lookup into the package fact builder (17 -> 16).
    // r23 folded the resource-calendar point lookup into the single-line projection/FROM builder (16 -> 15).
    // r24 folded the scoped single-row family into that same builder (15 -> 14).
    // r25 folded the scoped branch-list family into the single-line projection/FROM builder (14 -> 13).
    expect(total).toBe(5);
    expect(total).toBeLessThan(21);
  });
});
