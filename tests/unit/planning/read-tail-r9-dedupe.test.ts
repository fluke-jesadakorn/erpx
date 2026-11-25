import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe renderings from HEAD for the Planning command-source
// single-row readers. Byte-identity oracles only, never live query sources.
// Lane 36 retired both TS reads; their sole authority is now the native
// Planning owner route, so these strings are historical oracles only.
const SCENARIO = 'SELECT * FROM planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)';
const SCOPED_RECOMMENDATION = 'SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)';

describe('Planning read-tail r9 parameterized branch-scoped single-row family', () => {
  test('retired row reads are now served by the native owner route', () => {
    const text = src('production-command-sources-read.ts');
    for (const literal of [SCENARIO, SCOPED_RECOMMENDATION]) expect(text).not.toContain(literal);
    expect(text).not.toContain('planningScopedBranchRow(');
    expect(text).toContain("from './native-lane-36/client'");
    expect(text).toContain('nativeScenario({id},companyId,branchId)');
    expect(text).toContain('nativeScopedRecommendation({id},companyId,branchId)');
    expect(text).toContain('planningScopedBranchList('); // r8 family untouched
    expect(text).toContain('planningCommandExistsGuard('); // r7 family untouched
    expect(existsSync(resolve(srcRoot, 'adapters/planning-scoped-row-sql.ts'))).toBe(false);
  });

  test('the historical row oracles are read-only SELECTs', () => {
    for (const sql of [SCENARIO, SCOPED_RECOMMENDATION]) {
      expect(selectCount(sql)).toBe(1);
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r4-r8 helpers, native read-source consumers and all lock statements stay untouched', () => {
    const guard = src('planning-guard-sql.ts');
    expect(guard).toContain('export const productionResourcesExists =');
    expect(guard).toContain('export const planningAllocatedMinutes =');
    expect(guard).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('planning-scoped-list-sql.ts')).toContain('planningScopedBranchList');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('resources-read.ts')).toContain('calendarVersionGuard');
    expect(src('scenarios.ts')).toContain("from './production-command-sources-read'");
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('production-command-sources-read.ts')).toContain('NATIVE_PLANNING_REQUIRED');
  });

  test('retirement:ts-statements stays strictly below the r9 BEFORE=33 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(33);
    expect(total).toBeLessThanOrEqual(32);
  });
});
