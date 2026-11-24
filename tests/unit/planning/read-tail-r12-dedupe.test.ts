import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningJsonbRowByKey } from '../../../packages/server-planning/src/adapters/planning-jsonb-row-sql';
import { planningAttachmentScenarioSourceRows, planningAttachmentRecommendationSourceRows } from '../../../packages/server-planning/src/adapters/attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe subqueries from HEAD for the two attachment-lineage
// single-JSON-row lookups. Byte-identity oracles only, never live query sources.
const COMPARISON_ROW = "(SELECT to_jsonb(comparison) FROM planning_scenarios comparison WHERE comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id)";
const RECOMMENDATION_ROW = '(SELECT to_jsonb(recommendation) FROM planning_recommendations recommendation WHERE recommendation.scenario_id=scenario.id)';

const generated = [
  planningJsonbRowByKey('planning_scenarios', 'comparison', "comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id"),
  planningJsonbRowByKey('planning_recommendations', 'recommendation', 'recommendation.scenario_id=scenario.id'),
];

const EXPECTED_SCENARIO_SOURCE = `SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,
 jsonb_build_object('scenario',to_jsonb(scenario),
 'comparison',${COMPARISON_ROW},
 'recommendation',${RECOMMENDATION_ROW},
 'resources',(SELECT jsonb_agg(jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),
 'calendar',(SELECT jsonb_agg(to_jsonb(calendar) ORDER BY calendar.work_date) FROM production_resource_calendar calendar WHERE calendar.resource_id=resource.id)) ORDER BY allocation.id)
 FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id WHERE allocation.scenario_id=scenario.id)) image
 FROM planning_scenarios scenario`;

describe('Planning read-tail r12 parameterized attachment-lineage JSON-row family', () => {
  test('builder reproduces both originals byte-identically', () => {
    expect(generated).toEqual([COMPARISON_ROW, RECOMMENDATION_ROW]);
  });

  test('exported scenario source SQL is byte-identical to the pre-dedupe literal', () => {
    expect(planningAttachmentScenarioSourceRows).toBe(EXPECTED_SCENARIO_SOURCE);
    expect(planningAttachmentRecommendationSourceRows).toBe(`SELECT recommendation.id,recommendation.scenario_id,scenario.company_id,
 jsonb_build_object('recommendation',to_jsonb(recommendation),'scenario',scenario.image) image
 FROM planning_recommendations recommendation LEFT JOIN (${EXPECTED_SCENARIO_SOURCE}) scenario ON scenario.id=recommendation.scenario_id`);
  });

  test('both former sites compose the builder and keep their own exports', () => {
    const attachment = src('attachment-read-sources.ts');
    expect(attachment).not.toContain(COMPARISON_ROW);
    expect(attachment).not.toContain(RECOMMENDATION_ROW);
    expect(attachment).toContain("planningJsonbRowByKey('planning_scenarios','comparison',");
    expect(attachment).toContain("planningJsonbRowByKey('planning_recommendations','recommendation','recommendation.scenario_id=scenario.id')");
    expect(attachment).toContain('planning-jsonb-row-sql');
    expect(attachment).toContain('export const planningAttachmentScenarioSourceRows=');
    expect(attachment).toContain('export const planningAttachmentRecommendationSourceRows=');
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-jsonb-row-sql.ts');
    expect(selectCount(shared)).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of generated) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r4-r11 helpers, native consumers and all lock statements stay untouched', () => {
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('planning-scoped-list-sql.ts')).toContain('planningScopedBranchList');
    expect(src('planning-scoped-row-sql.ts')).toContain('planningScopedBranchRow');
    expect(src('planning-calendar-source-sql.ts')).toContain('resourceCalendarByKey');
    expect(src('planning-resource-allocation-sql.ts')).toContain('planningResourceAllocationRows');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('attachment-read-sources.ts')).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);

    const publicBarrel = readFileSync(resolve(srcRoot, 'public/attachment-read-sources.ts'), 'utf8');
    expect(publicBarrel).toContain('planningAttachmentScenarioSourceRows');
    expect(publicBarrel).toContain('planningAttachmentRecommendationSourceRows');

    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
  });

  test('retirement:ts-statements stays strictly below the r12 BEFORE=30 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(30);
    expect(total).toBeLessThanOrEqual(29);
  });
});
