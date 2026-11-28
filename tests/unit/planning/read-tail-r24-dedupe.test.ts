import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-r24 renderings from HEAD for the two scoped single-row lookups
// (scenario and scopedRecommendation). Byte-identity oracles only, never live
// query sources: lane 36 retired both TS reads and their sole authority is now
// the native Planning owner route.
const SCENARIO_ROW = 'SELECT * FROM planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)';
const RECOMMENDATION_ROW = 'SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)';

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Planning read-tail r24 scoped single-row head fold', () => {
  test('retired row SQL is now owned by the native Planning route', () => {
    const sources = src('production-command-sources-read.ts');
    expect(sources).not.toContain(SCENARIO_ROW);
    expect(sources).not.toContain(RECOMMENDATION_ROW);
    expect(sources).not.toContain('planningScopedBranchRow(');
    expect(sources).toContain('nativeScenario({id},companyId,branchId)');
    expect(sources).toContain('nativeScopedRecommendation({id},companyId,branchId)');
    for (const sql of [SCENARIO_ROW, RECOMMENDATION_ROW]) {
      expect(selectCount(sql)).toBe(1);
      readOnly(sql);
    }
  });

  test('the retired wrapper is gone while the generalized builder stays read-only', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/planning-scoped-row-sql.ts'))).toBe(false);
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
  });

  test('r1-r23 helpers, the single lock statement and native readers stay untouched', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
    expect(readdirSync(resolve(srcRoot, 'adapters'))).not.toContain('planning-select-from-sql.ts');
    expect(src('planning-package-fact-sql.ts')).toContain('export const planningPackageFactRows =');
    expect(existsSync(resolve(srcRoot, 'adapters/planning-scoped-row-sql.ts'))).toBe(false);
    expect(src('planning-scoped-list-sql.ts')).toContain('export const planningScopedBranchList =');
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('planning-image-source-sql.ts')).toContain('planningSourceRows');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    let locks = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) locks += (readFileSync(path, 'utf8').match(/LOCK TABLE/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(locks).toBe(1);
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts', 'capacity.ts']) {
      expect(src(name)).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    }
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningScopedBranchRow');
        expect(text).not.toContain('planningSelectColumnsFrom');
      }
    }
  });

  test('retirement:ts-statements strictly lowers the r23 BEFORE=15 baseline to 14', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r25 folded the scoped branch-list family into the single-line projection/FROM builder (14 -> 13).
    expect(total).toBe(5);
    expect(total).toBeLessThan(14);
  });
});
