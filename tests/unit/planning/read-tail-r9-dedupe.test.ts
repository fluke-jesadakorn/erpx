import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningScopedBranchRow } from '../../../packages/server-planning/src/adapters/planning-scoped-row-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe renderings from HEAD for the Planning command-source
// single-row readers. Byte-identity oracles only, never live query sources.
const SCENARIO = 'SELECT * FROM planning_scenarios WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?)';
const SCOPED_RECOMMENDATION = 'SELECT r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE r.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?)';

const generated = [
  planningScopedBranchRow('*', 'planning_scenarios', 'id', 'company_id', 'branch_id'),
  planningScopedBranchRow('r.*,s.company_id,s.branch_id,s.warehouse_id,s.specification_id,s.specification_version,s.context_json,s.created_by scenario_created_by', 'planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id', 'r.id', 's.company_id', 's.branch_id'),
];

describe('Planning read-tail r9 parameterized branch-scoped single-row family', () => {
  test('row builder reproduces both originals byte-identically', () => {
    expect(generated).toEqual([SCENARIO, SCOPED_RECOMMENDATION]);
  });

  test('every former row site composes the builder and keeps its own prepare/bind', () => {
    const text = src('production-command-sources-read.ts');
    for (const literal of [SCENARIO, SCOPED_RECOMMENDATION]) expect(text).not.toContain(literal);
    expect(text.match(/planningScopedBranchRow\(/g) ?? []).toHaveLength(2);
    expect(text).toContain('planning-scoped-row-sql');
    expect(text).toContain('.bind(id, companyId, branchId, branchId2)');
    expect(text).toContain('planningScopedBranchList('); // r8 family untouched
    expect(text).toContain('planningCommandExistsGuard('); // r7 family untouched
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-scoped-row-sql.ts');
    expect(selectCount(shared)).toBe(1);
    for (const sql of generated) {
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
