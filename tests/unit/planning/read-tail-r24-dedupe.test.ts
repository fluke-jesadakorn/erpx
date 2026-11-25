import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningScopedBranchRow } from '../../../packages/server-planning/src/adapters/planning-scoped-row-sql';
import { planningSelectColumnsFrom } from '../../../packages/server-planning/src/adapters/planning-select-columns-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-r24 renderings from HEAD for the two scoped single-row lookups
// (scenario and scopedRecommendation). Byte-identity oracles only, never live
// query sources: the thin wrapper must emit these exactly after folding into
// the generalized single-line projection/FROM builder.
const SCENARIO_ROW = 'SELECT * FROM planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)';
const RECOMMENDATION_ROW = 'SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)';
const SCENARIO_TAIL = 'planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)';
const RECOMMENDATION_TAIL = 'planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)';

const generated = [
  planningScopedBranchRow('*', 'planning_scenarios', 'id', 'company_id', 'branch_id'),
  planningScopedBranchRow('r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by', 'planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id', 'r.id', 's.company_id', 's.branch_id'),
];

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Planning read-tail r24 scoped single-row head fold', () => {
  test('thin wrapper reproduces both pre-r24 statements byte-identically', () => {
    expect(generated).toEqual([SCENARIO_ROW, RECOMMENDATION_ROW]);
    expect(generated[0]).toBe(planningSelectColumnsFrom('*', SCENARIO_TAIL));
    expect(generated[1]).toBe(planningSelectColumnsFrom('r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by', RECOMMENDATION_TAIL));
    generated.forEach(readOnly);
  });

  test('wrapper delegates and the generalized builder owns the single read literal', () => {
    const wrapper = src('planning-scoped-row-sql.ts');
    expect(selectCount(wrapper)).toBe(0);
    expect(wrapper).toContain('planningSelectColumnsFrom(columns, `${from} WHERE ${id}=? AND ${company}=? AND (?::text IS NULL OR ${branch}=?)`)');
    expect(wrapper).not.toContain('SELECT ');
    expect(wrapper.match(/export const planningScopedBranchRow =/g) ?? []).toHaveLength(1);
    const head = src('planning-select-columns-sql.ts');
    expect(selectCount(head)).toBe(1);
    expect(head).toContain('`SELECT ${columns} FROM ${rest}`');
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('both callers keep composing the wrapper with unchanged binds', () => {
    const sources = src('production-command-sources-read.ts');
    expect(sources).toContain("planningScopedBranchRow('*', 'planning_scenarios', 'id', 'company_id', 'branch_id')");
    expect(sources).toContain('.bind(id, companyId, branchId, branchId2).first<Row>()');
    expect(sources).toContain("planningScopedBranchRow('r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by', 'planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id', 'r.id', 's.company_id', 's.branch_id')");
    expect(sources).toContain('planning-scoped-row-sql');
    expect(sources).not.toContain(SCENARIO_ROW);
    expect(sources).not.toContain(RECOMMENDATION_ROW);
  });

  test('r1-r23 helpers, the single lock statement and native readers stay untouched', () => {
    expect(src('planning-select-columns-sql.ts')).toContain('export const planningSelectColumnsFrom =');
    expect(src('planning-select-from-sql.ts')).toContain('export const planningSelectFrom =');
    expect(src('planning-package-fact-sql.ts')).toContain('export const planningPackageFactRows =');
    expect(src('planning-scoped-row-sql.ts')).toContain('export const planningScopedBranchRow =');
    expect(src('planning-scoped-list-sql.ts')).toContain('export const planningScopedBranchList =');
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('export const planningCommandExistsGuard =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetRow =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetVersionExists =');
    expect(src('planning-image-source-sql.ts')).toContain('export const planningSourceRows =');
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
    // r23 folded the resource-calendar point lookup into the single-line projection/FROM builder (16 -> 15).
    // r24 folded the scoped single-row family into that same builder (15 -> 14).
    // r25 folded the scoped branch-list family into the single-line projection/FROM builder (14 -> 13).
    expect(total).toBe(13);
    expect(total).toBeLessThan(14);
  });
});
