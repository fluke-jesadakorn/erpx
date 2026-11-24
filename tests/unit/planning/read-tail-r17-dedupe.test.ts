import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningSelectFrom } from '../../../packages/server-planning/src/adapters/planning-select-from-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe declarations and SQL from the r16 BEFORE=22 source. These
// are byte-identity oracles only, never live query sources. The LATERAL shortage
// projection and the main executive projection share one parameterized
// projection/FROM head: only the fixed columns, indent and FROM fragment differ.
const ORIG_SHORTAGE_DECL = "const shortageProjection = `\n    LEFT JOIN LATERAL (\n      SELECT COALESCE(jsonb_agg(jsonb_build_object(\n        'product_name',COALESCE(p.name,'Unknown product'),\n        'required_milli',(entry.value->>'requiredMilli')::numeric::text,\n        'available_milli',(entry.value->>'availableMilli')::numeric::text,\n        'shortage_milli',(entry.value->>'shortageMilli')::numeric::text\n      ) ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,\n      count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count\n      FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)\n      ${productioncommerceProjection.executiveShortageProduct}\n    ) shortage ON TRUE`;";
const ORIG_PROJECTION_DECL = "const projection = `SELECT s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,\n      s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,\n      spec.output_product_id,output.name output_product_name,s.specification_version,\n      s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,\n      s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,\n      r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,\n      r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,\n      r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,\n      COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json\n    FROM planning_scenarios s ${sourceJoins} ${shortageProjection}`;";
const NEW_SHORTAGE_DECL = "const shortageProjection = `\n    LEFT JOIN LATERAL (\n      ${planningSelectFrom(`COALESCE(jsonb_agg(jsonb_build_object(\n        'product_name',COALESCE(p.name,'Unknown product'),\n        'required_milli',(entry.value->>'requiredMilli')::numeric::text,\n        'available_milli',(entry.value->>'availableMilli')::numeric::text,\n        'shortage_milli',(entry.value->>'shortageMilli')::numeric::text\n      ) ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,\n      count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count`, '      ', `jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)\n      ${productioncommerceProjection.executiveShortageProduct}`)}\n    ) shortage ON TRUE`;";
const NEW_PROJECTION_DECL = "const projection = planningSelectFrom(`s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,\n      s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,\n      spec.output_product_id,output.name output_product_name,s.specification_version,\n      s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,\n      s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,\n      r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,\n      r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,\n      r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,\n      COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json`, '    ', `planning_scenarios s ${sourceJoins} ${shortageProjection}`);";

const ORIG_SHORTAGE_SQL = "SELECT COALESCE(jsonb_agg(jsonb_build_object(\n        'product_name',COALESCE(p.name,'Unknown product'),\n        'required_milli',(entry.value->>'requiredMilli')::numeric::text,\n        'available_milli',(entry.value->>'availableMilli')::numeric::text,\n        'shortage_milli',(entry.value->>'shortageMilli')::numeric::text\n      ) ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,\n      count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count\n      FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)\n      ${productioncommerceProjection.executiveShortageProduct}";
const ORIG_PROJECTION_SQL = "SELECT s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,\n      s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,\n      spec.output_product_id,output.name output_product_name,s.specification_version,\n      s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,\n      s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,\n      r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,\n      r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,\n      r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,\n      COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json\n    FROM planning_scenarios s ${sourceJoins} ${shortageProjection}";

// Fixed fragments mirror the source expressions; both original oracles and
// generated SQL keep the same literal interpolation text, so byte-identity is
// checked on the emitted template, not on cross-owner fragment values.
const sourceJoins = '${sourceJoins}';
const shortageProjection = '${shortageProjection}';
const productioncommerceProjection = { executiveShortageProduct: '${productioncommerceProjection.executiveShortageProduct}' };

const generatedShortage = planningSelectFrom(`COALESCE(jsonb_agg(jsonb_build_object(
        'product_name',COALESCE(p.name,'Unknown product'),
        'required_milli',(entry.value->>'requiredMilli')::numeric::text,
        'available_milli',(entry.value->>'availableMilli')::numeric::text,
        'shortage_milli',(entry.value->>'shortageMilli')::numeric::text
      ) ORDER BY COALESCE(p.name,entry.value->>'productId')) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0),'[]'::jsonb)::text shortage_items_json,
      count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count`, '      ', `jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)
      ${productioncommerceProjection.executiveShortageProduct}`);
const generatedProjection = planningSelectFrom(`s.id scenario_id,s.company_id,s.branch_id,branch.display_name branch_name,
      s.warehouse_id,w.name warehouse_name,s.specification_id,spec.name specification_name,
      spec.output_product_id,output.name output_product_name,s.specification_version,
      s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,
      s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,
      r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,
      r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,
      r.scenario_version,r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,
      COALESCE(shortage.shortage_count,'0') shortage_count,COALESCE(shortage.shortage_items_json,'[]') shortage_items_json`, '    ', `planning_scenarios s ${sourceJoins} ${shortageProjection}`);

describe('Planning read-tail r17 parameterized projection/FROM head', () => {
  test('head reproduces both former inline statements byte-identically', () => {
    expect(generatedShortage).toBe(ORIG_SHORTAGE_SQL);
    expect(generatedProjection).toBe(ORIG_PROJECTION_SQL);
  });

  test('restoring the builder calls yields the verbatim pre-dedupe declarations', () => {
    let restored = src('production-command-sources-read.ts');
    expect(restored).toContain(NEW_SHORTAGE_DECL);
    expect(restored).toContain(NEW_PROJECTION_DECL);
    restored = restored.replace(NEW_SHORTAGE_DECL, ORIG_SHORTAGE_DECL).replace(NEW_PROJECTION_DECL, ORIG_PROJECTION_DECL);
    expect(restored).toContain(ORIG_SHORTAGE_DECL);
    expect(restored).toContain(ORIG_PROJECTION_DECL);
    expect(restored).not.toContain(NEW_SHORTAGE_DECL);
    expect(restored).not.toContain(NEW_PROJECTION_DECL);
    const current = src('production-command-sources-read.ts');
    expect(current).not.toContain(ORIG_SHORTAGE_SQL);
    expect(current).not.toContain(ORIG_PROJECTION_SQL);
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-select-from-sql.ts');
    expect(selectCount(shared)).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of [generatedShortage, generatedProjection])
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|PG_ADVISORY/i);
  });

  test('r1-r16 helpers, native reads and all lock statements stay untouched', () => {
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
    expect(src('planning-image-source-sql.ts')).toContain('planningSourceRows');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('package-obligations.ts')).toContain('planningPurchaseSourceReadLocks(database)');
    const read = src('production-command-sources-read.ts');
    expect(read).toContain('planningScopedBranchList(');
    expect(read).toContain('planningScopedBranchRow(');
    expect(read).toContain('planningCommandExistsGuard(');
    expect(read).not.toContain('LOCK TABLE');
    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningSelectFrom');
      }
    }
  });

  test('retirement:ts-statements stays strictly below the r16 BEFORE=22 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r17 folds the LATERAL shortage and main executive projection heads into
    // one owner-local builder (22 -> 21) while keeping every SQL byte-identical;
    // r18 folds the calendar/comparison single-line heads (21 -> 20);
    // r19 folds the capacity operations head into this builder (20 -> 19);
    // r20 folds the production-resources guard into this EXISTS builder (19 -> 18).
    expect(total).toBe(18);
    expect(total).toBeLessThan(21);
  });
});
