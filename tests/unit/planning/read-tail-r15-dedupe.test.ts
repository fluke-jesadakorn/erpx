import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningImageSourceRows } from '../../../packages/server-planning/src/adapters/planning-image-source-sql';
import { planningJsonbRowByKey } from '../../../packages/server-planning/src/adapters/planning-jsonb-row-sql';
import { planningJsonbAggOrdered } from '../../../packages/server-planning/src/adapters/planning-jsonb-agg-sql';
import { planningAttachmentScenarioSourceRows, planningAttachmentRecommendationSourceRows } from '../../../packages/server-planning/src/adapters/attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe image-source literals from HEAD (byte-identity oracles
// only, never live query sources). Both sources have the same head --
// "SELECT <columns>,\n <image> image\n FROM <from>" -- and differ only in the
// fixed server-owned columns/image/FROM fragments, so one owner-local head now
// emits both. r1-r14 deduped the nested jsonb row/agg fragments, not this head.
const ORIG_SCENARIO = `SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,
 jsonb_build_object('scenario',to_jsonb(scenario),
 'comparison',${planningJsonbRowByKey('planning_scenarios','comparison',"comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id")},
 'recommendation',${planningJsonbRowByKey('planning_recommendations','recommendation','recommendation.scenario_id=scenario.id')},
 'resources',${planningJsonbAggOrdered(`jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\n 'calendar',${planningJsonbAggOrdered('to_jsonb(calendar)','calendar.work_date',' FROM production_resource_calendar calendar','calendar.resource_id=resource.id')})`,'allocation.id','\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id','allocation.scenario_id=scenario.id')}) image
 FROM planning_scenarios scenario`;
const ORIG_RECOMMENDATION = `SELECT recommendation.id,recommendation.scenario_id,scenario.company_id,
 jsonb_build_object('recommendation',to_jsonb(recommendation),'scenario',scenario.image) image
 FROM planning_recommendations recommendation LEFT JOIN (${ORIG_SCENARIO}) scenario ON scenario.id=recommendation.scenario_id`;

const generatedScenario = planningImageSourceRows(
  'scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id',
  `jsonb_build_object('scenario',to_jsonb(scenario),
 'comparison',${planningJsonbRowByKey('planning_scenarios','comparison',"comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id")},
 'recommendation',${planningJsonbRowByKey('planning_recommendations','recommendation','recommendation.scenario_id=scenario.id')},
 'resources',${planningJsonbAggOrdered(`jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\n 'calendar',${planningJsonbAggOrdered('to_jsonb(calendar)','calendar.work_date',' FROM production_resource_calendar calendar','calendar.resource_id=resource.id')})`,'allocation.id','\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id','allocation.scenario_id=scenario.id')})`,
  'planning_scenarios scenario',
);
const generatedRecommendation = planningImageSourceRows(
  'recommendation.id,recommendation.scenario_id,scenario.company_id',
  `jsonb_build_object('recommendation',to_jsonb(recommendation),'scenario',scenario.image)`,
  `planning_recommendations recommendation LEFT JOIN (${ORIG_SCENARIO}) scenario ON scenario.id=recommendation.scenario_id`,
);

describe('Planning read-tail r15 parameterized image-source row family', () => {
  test('head reproduces both pre-dedupe sources byte-identically', () => {
    expect(generatedScenario).toBe(ORIG_SCENARIO);
    expect(generatedRecommendation).toBe(ORIG_RECOMMENDATION);
  });

  test('live exports are byte-identical to the pre-dedupe literals', () => {
    expect(planningAttachmentScenarioSourceRows).toBe(ORIG_SCENARIO);
    expect(planningAttachmentRecommendationSourceRows).toBe(ORIG_RECOMMENDATION);
  });

  test('both former sites compose the head and keep their exports and nested fragments', () => {
    const attachment = src('attachment-read-sources.ts');
    expect(selectCount(attachment)).toBe(0);
    expect(attachment.match(/planningImageSourceRows\(/g) ?? []).toHaveLength(2);
    expect(attachment).toContain('export const planningAttachmentScenarioSourceRows=');
    expect(attachment).toContain('export const planningAttachmentRecommendationSourceRows=');
    expect(attachment).toContain("planningJsonbRowByKey('planning_scenarios','comparison',");
    expect(attachment).toContain("planningJsonbRowByKey('planning_recommendations','recommendation','recommendation.scenario_id=scenario.id')");
    // r14's nested ordered jsonb_agg call is preserved verbatim inside the image fragment.
    expect(attachment).toContain("planningJsonbAggOrdered(`jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\\n 'calendar',${planningJsonbAggOrdered('to_jsonb(calendar)','calendar.work_date',' FROM production_resource_calendar calendar','calendar.resource_id=resource.id')})`,'allocation.id','\\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id','allocation.scenario_id=scenario.id')})");
    expect(attachment).toContain('planning-image-source-sql');
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-image-source-sql.ts');
    expect(selectCount(shared)).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of [ORIG_SCENARIO, ORIG_RECOMMENDATION]) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r1-r14 helpers, native reads and all lock statements stay untouched', () => {
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
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('package-obligations.ts')).toContain('planningPurchaseSourceReadLocks(database)');

    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) expect(readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8')).not.toContain('planningImageSourceRows');
    }
  });

  test('retirement:ts-statements stays strictly below the r14 BEFORE=26 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r16 folded the three purchase witness rows into the generalized source-row
    // head (25 -> 22); r17 folded the shortage/executive projection heads (22 -> 21)
    // while keeping every emitted SQL byte-identical; r18 folded the calendar/comparison
    // single-line heads (21 -> 20); r19 folded the capacity operations head into
    // the r17 builder (20 -> 19); r20 folded the production-resources guard
    // into the r7 EXISTS builder (19 -> 18).
    // r22 folded the resource-allocation point lookup into the package fact builder (17 -> 16).
    // r23 folded the resource-calendar point lookup into the single-line projection/FROM builder (16 -> 15).
    // r24 folded the scoped single-row family into that same builder (15 -> 14).
    // r25 folded the scoped branch-list family into the single-line projection/FROM builder (14 -> 13).
    expect(total).toBe(13);
    expect(total).toBeLessThan(26);
  });
});
