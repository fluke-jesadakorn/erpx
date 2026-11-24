import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningSourceRows, planningImageSourceRows } from '../../../packages/server-planning/src/adapters/planning-image-source-sql';
import { planningPurchaseScenarioRows, planningPurchaseRecommendationRows, planningPurchaseResourceRows, planningPurchaseSourceReadLocks } from '../../../packages/server-planning/src/adapters/purchase-sources';
import { planningAttachmentScenarioSourceRows, planningAttachmentRecommendationSourceRows } from '../../../packages/server-planning/src/adapters/attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe SQL from the r15 BEFORE=25 source (byte-identity
// oracles only, never live query sources). All three purchase witness rows and
// both r15 image rows share one generalized owner-local source-row head; only
// the fixed columns, projection, alias, FROM fragment and whitespace differ.
const ORIG_SCENARIO = "SELECT s.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,\n (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')<>'' OR jsonb_exists(s.context_json::jsonb,'comparison')) advanced,\n jsonb_build_object('scenario',to_jsonb(s)-'status'-'version'-'updated_at','comparison',CASE WHEN comparison.id IS NULL THEN NULL ELSE jsonb_build_array(comparison.id,comparison.company_id,comparison.branch_id,comparison.specification_id,comparison.version) END) witness\n FROM planning_scenarios s LEFT JOIN planning_scenarios comparison ON comparison.id=s.context_json::jsonb->>'comparisonScenarioId'\n AND comparison.id=s.context_json::jsonb->'comparison'->>'scenarioId' AND comparison.company_id=s.company_id AND comparison.branch_id=s.branch_id AND comparison.specification_id=s.specification_id\n AND comparison.version>=(s.context_json::jsonb->'comparison'->>'sourceVersion')::bigint\n WHERE (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')='' AND NOT(jsonb_exists(s.context_json::jsonb,'comparison'))) OR comparison.id IS NOT NULL";
const ORIG_RECOMMENDATION = "SELECT r.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.advanced,\n jsonb_build_object('recommendation',to_jsonb(r)-'status'-'version'-'updated_at'-'reviewed_by','scenario',s.witness) witness\n FROM planning_recommendations r JOIN (SELECT s.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,\n (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')<>'' OR jsonb_exists(s.context_json::jsonb,'comparison')) advanced,\n jsonb_build_object('scenario',to_jsonb(s)-'status'-'version'-'updated_at','comparison',CASE WHEN comparison.id IS NULL THEN NULL ELSE jsonb_build_array(comparison.id,comparison.company_id,comparison.branch_id,comparison.specification_id,comparison.version) END) witness\n FROM planning_scenarios s LEFT JOIN planning_scenarios comparison ON comparison.id=s.context_json::jsonb->>'comparisonScenarioId'\n AND comparison.id=s.context_json::jsonb->'comparison'->>'scenarioId' AND comparison.company_id=s.company_id AND comparison.branch_id=s.branch_id AND comparison.specification_id=s.specification_id\n AND comparison.version>=(s.context_json::jsonb->'comparison'->>'sourceVersion')::bigint\n WHERE (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')='' AND NOT(jsonb_exists(s.context_json::jsonb,'comparison'))) OR comparison.id IS NOT NULL) s ON s.id=r.scenario_id";
const ORIG_RESOURCE = "SELECT id,company_id,branch_id,false advanced,jsonb_build_array(id,company_id,branch_id,created_by,created_at) witness FROM production_resources";
const ORIG_ATTACHMENT_SCENARIO = "SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,\n jsonb_build_object('scenario',to_jsonb(scenario),\n 'comparison',(SELECT to_jsonb(comparison) FROM planning_scenarios comparison WHERE comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id),\n 'recommendation',(SELECT to_jsonb(recommendation) FROM planning_recommendations recommendation WHERE recommendation.scenario_id=scenario.id),\n 'resources',(SELECT jsonb_agg(jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\n 'calendar',(SELECT jsonb_agg(to_jsonb(calendar) ORDER BY calendar.work_date) FROM production_resource_calendar calendar WHERE calendar.resource_id=resource.id)) ORDER BY allocation.id)\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id WHERE allocation.scenario_id=scenario.id)) image\n FROM planning_scenarios scenario";
const ORIG_ATTACHMENT_RECOMMENDATION = "SELECT recommendation.id,recommendation.scenario_id,scenario.company_id,\n jsonb_build_object('recommendation',to_jsonb(recommendation),'scenario',scenario.image) image\n FROM planning_recommendations recommendation LEFT JOIN (SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,\n jsonb_build_object('scenario',to_jsonb(scenario),\n 'comparison',(SELECT to_jsonb(comparison) FROM planning_scenarios comparison WHERE comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id),\n 'recommendation',(SELECT to_jsonb(recommendation) FROM planning_recommendations recommendation WHERE recommendation.scenario_id=scenario.id),\n 'resources',(SELECT jsonb_agg(jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\n 'calendar',(SELECT jsonb_agg(to_jsonb(calendar) ORDER BY calendar.work_date) FROM production_resource_calendar calendar WHERE calendar.resource_id=resource.id)) ORDER BY allocation.id)\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id WHERE allocation.scenario_id=scenario.id)) image\n FROM planning_scenarios scenario) scenario ON scenario.id=recommendation.scenario_id";

const generatedScenario = planningSourceRows(
 `s.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,
 (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')<>'' OR jsonb_exists(s.context_json::jsonb,'comparison')) advanced`,
 `jsonb_build_object('scenario',to_jsonb(s)-'status'-'version'-'updated_at','comparison',CASE WHEN comparison.id IS NULL THEN NULL ELSE jsonb_build_array(comparison.id,comparison.company_id,comparison.branch_id,comparison.specification_id,comparison.version) END)`,
 'witness',
 `planning_scenarios s LEFT JOIN planning_scenarios comparison ON comparison.id=s.context_json::jsonb->>'comparisonScenarioId'
 AND comparison.id=s.context_json::jsonb->'comparison'->>'scenarioId' AND comparison.company_id=s.company_id AND comparison.branch_id=s.branch_id AND comparison.specification_id=s.specification_id
 AND comparison.version>=(s.context_json::jsonb->'comparison'->>'sourceVersion')::bigint
 WHERE (COALESCE(s.context_json::jsonb->>'comparisonScenarioId','')='' AND NOT(jsonb_exists(s.context_json::jsonb,'comparison'))) OR comparison.id IS NOT NULL`);
const generatedRecommendation = planningSourceRows(
 'r.id,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.advanced',
 `jsonb_build_object('recommendation',to_jsonb(r)-'status'-'version'-'updated_at'-'reviewed_by','scenario',s.witness)`,
 'witness',
 `planning_recommendations r JOIN (${ORIG_SCENARIO}) s ON s.id=r.scenario_id`);
const generatedResource = planningSourceRows(
 'id,company_id,branch_id,false advanced',
 'jsonb_build_array(id,company_id,branch_id,created_by,created_at)',
 'witness',
 'production_resources',
 ',',
 ' FROM ');
const generatedAttachmentScenario = planningImageSourceRows(
 'scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id',
 'placeholder-image',
 'planning_scenarios scenario');

describe('Planning read-tail r16 generalized source-row head', () => {
  test('head reproduces every former purchase witness row byte-identically', () => {
    expect(generatedScenario).toBe(ORIG_SCENARIO);
    // The recommendation source embeds the generated scenario row, exactly as
    // the former inline literal embedded the former scenario literal.
    expect(generatedRecommendation).toBe(ORIG_RECOMMENDATION.replace(ORIG_SCENARIO, generatedScenario));
    expect(generatedResource).toBe(ORIG_RESOURCE);
  });

  test('live purchase and r15 exports are byte-identical to their pre-dedupe literals', () => {
    expect(planningPurchaseScenarioRows).toBe(ORIG_SCENARIO);
    expect(planningPurchaseRecommendationRows).toBe(ORIG_RECOMMENDATION);
    expect(planningPurchaseResourceRows).toBe(ORIG_RESOURCE);
    expect(planningAttachmentScenarioSourceRows).toBe(ORIG_ATTACHMENT_SCENARIO);
    expect(planningAttachmentRecommendationSourceRows).toBe(ORIG_ATTACHMENT_RECOMMENDATION);
  });

  test('r15 image head is still emitted verbatim through the generalized head', () => {
    expect(planningImageSourceRows('a,b', 'img', 'src')).toBe('SELECT a,b,\n img image\n FROM src');
    expect(generatedAttachmentScenario).toBe('SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,\n placeholder-image image\n FROM planning_scenarios scenario');
  });

  test('both families compose the head; binds and LOCK TABLE stay untouched', () => {
    const purchase = src('purchase-sources.ts');
    expect(selectCount(purchase)).toBe(0);
    expect(purchase.match(/planningSourceRows\(/g) ?? []).toHaveLength(3);
    expect(purchase).toContain('export const planningPurchaseScenarioRows=planningSourceRows(');
    expect(purchase).toContain('export const planningPurchaseRecommendationRows=planningSourceRows(');
    expect(purchase).toContain('export const planningPurchaseResourceRows=planningSourceRows(');
    expect(purchase).toContain("LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?\"SHARE ROW EXCLUSIVE\":\"SHARE\"} MODE");
    expect(purchase).not.toContain('.bind(');
    expect(src('package-obligations.ts')).toContain('.bind(input.companyId,input.packageId==="manufacturing")');
    expect(src('package-obligations.ts')).toContain('planningPurchaseSourceReadLocks(database)');
    expect(planningPurchaseSourceReadLocks({ prepare: (sql: string) => sql } as never)).toEqual(['LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN SHARE MODE']);
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-image-source-sql.ts');
    expect(selectCount(shared)).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of [generatedScenario, generatedRecommendation, generatedResource, generatedAttachmentScenario]) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|PG_ADVISORY/i);
    }
  });

  test('r1-r15 helpers, native reads and all lock statements stay untouched', () => {
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('planning-scoped-list-sql.ts')).toContain('planningScopedBranchList');
    expect(src('planning-scoped-row-sql.ts')).toContain('planningScopedBranchRow');
    expect(src('planning-calendar-source-sql.ts')).toContain('resourceCalendarByKey');
    expect(src('planning-resource-allocation-sql.ts')).toContain('planningResourceAllocationRows');
    expect(src('planning-jsonb-row-sql.ts')).toContain('planningJsonbRowByKey');
    expect(src('planning-jsonb-agg-sql.ts')).toContain('planningJsonbAggOrdered');
    expect(src('planning-package-fact-sql.ts')).toContain('planningPackageFactRows');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');

    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningSourceRows');
        expect(text).not.toContain('planningImageSourceRows');
      }
    }
  });

  test('retirement:ts-statements stays strictly below the r15 BEFORE=25 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r16: the two canonical purchase witness rows (-2) and the compact
    // resource witness row folded into the generalized r15 head (-1);
    // r17 folded the shortage/executive projection heads (22 -> 21).
    expect(total).toBe(21);
    expect(total).toBeLessThan(25);
  });
});
