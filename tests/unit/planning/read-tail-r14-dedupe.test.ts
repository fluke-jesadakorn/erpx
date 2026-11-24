import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningJsonbAggOrdered } from '../../../packages/server-planning/src/adapters/planning-jsonb-agg-sql';
import { planningJsonbRowByKey } from '../../../packages/server-planning/src/adapters/planning-jsonb-row-sql';
import { planningAttachmentScenarioSourceRows } from '../../../packages/server-planning/src/adapters/attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe ordered jsonb_agg subqueries from HEAD (byte-identity
// oracles only, never live query sources). The inner calendar aggregate and the
// outer resource aggregate differ only in projection, order, source and
// predicate, so one owner-local builder now emits both.
const ORIG_CALENDAR = `(SELECT jsonb_agg(to_jsonb(calendar) ORDER BY calendar.work_date) FROM production_resource_calendar calendar WHERE calendar.resource_id=resource.id)`;
const ORIG_RESOURCES = `(SELECT jsonb_agg(jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),
 'calendar',${ORIG_CALENDAR}) ORDER BY allocation.id)
 FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id WHERE allocation.scenario_id=scenario.id)`;
// The enclosing jsonb_build_object from the scenario header adds one more close.
const ORIG_FULL = `${ORIG_RESOURCES})`;

const generatedCalendar = planningJsonbAggOrdered('to_jsonb(calendar)', 'calendar.work_date', ' FROM production_resource_calendar calendar', 'calendar.resource_id=resource.id');
const generatedResources = planningJsonbAggOrdered(
  `jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\n 'calendar',${generatedCalendar})`,
  'allocation.id',
  '\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id',
  'allocation.scenario_id=scenario.id',
);

const OUTER_CALL = `\${planningJsonbAggOrdered(\`jsonb_build_object('allocation',to_jsonb(allocation),'resource',to_jsonb(resource),\\n 'calendar',\${planningJsonbAggOrdered('to_jsonb(calendar)','calendar.work_date',' FROM production_resource_calendar calendar','calendar.resource_id=resource.id')})\`,'allocation.id','\\n FROM planning_resource_allocations allocation LEFT JOIN production_resources resource ON resource.id=allocation.resource_id AND resource.company_id=scenario.company_id','allocation.scenario_id=scenario.id')})`;

const EXPECTED_SCENARIO_SQL = `SELECT scenario.id,scenario.company_id,scenario.specification_id,scenario.warehouse_id,
 jsonb_build_object('scenario',to_jsonb(scenario),
 'comparison',${planningJsonbRowByKey('planning_scenarios','comparison',"comparison.id=CASE WHEN scenario.context_json IS JSON OBJECT THEN scenario.context_json::jsonb->>'comparisonScenarioId' END AND comparison.company_id=scenario.company_id")},
 'recommendation',${planningJsonbRowByKey('planning_recommendations','recommendation','recommendation.scenario_id=scenario.id')},
 'resources',${ORIG_FULL} image
 FROM planning_scenarios scenario`;

describe('Planning read-tail r14 parameterized ordered jsonb_agg family', () => {
  test('builder reproduces both originals byte-identically', () => {
    expect(generatedCalendar).toBe(ORIG_CALENDAR);
    expect(generatedResources).toBe(ORIG_RESOURCES);
  });

  test('live scenario source still emits the verbatim pre-dedupe statement', () => {
    expect(planningAttachmentScenarioSourceRows).toBe(EXPECTED_SCENARIO_SQL);
  });

  test('replacing the nested builder calls restores the verbatim pre-dedupe block', () => {
    let restored = src('attachment-read-sources.ts');
    expect(restored).toContain(OUTER_CALL);
    restored = restored.replace(OUTER_CALL, ORIG_FULL);
    // r15 moved the trailing " image" into the shared head helper; the nested
    // pre-dedupe aggregate is still restored verbatim in source and in output.
    expect(restored).toContain(` 'resources',${ORIG_FULL}`);
    expect(planningAttachmentScenarioSourceRows).toContain(` 'resources',${ORIG_FULL} image`);
    expect(restored).toContain(ORIG_CALENDAR);
    expect(src('attachment-read-sources.ts')).not.toContain(ORIG_FULL);
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-jsonb-agg-sql.ts');
    expect(selectCount(shared)).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of [generatedCalendar, generatedResources]) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r1-r13 helpers, native consumers and all lock statements stay untouched', () => {
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
    expect(src('planning-package-fact-sql.ts')).toContain('planningPackageFactRows');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('package-obligations.ts')).toContain('planningPurchaseSourceReadLocks(database)');

    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) expect(readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8')).not.toContain('planningJsonbAggOrdered');
    }
  });

  test('retirement:ts-statements stays strictly below the r13 BEFORE=27 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r15 shared the two attachment image-source heads through planningImageSourceRows (26 -> 25).
    expect(total).toBe(25);
    expect(total).toBeLessThan(27);
  });
});
