import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningScopedBranchList } from '../../../packages/server-planning/src/adapters/planning-scoped-list-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe renderings from HEAD, evaluated with a fixed scenario-spec
// fragment. Byte-identity oracles only, never live query sources.
const SPEC = '<SCENARIO_SPEC>';
const SCENARIOS = `SELECT p.*,s.name specification_name FROM planning_scenarios p ${SPEC} WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY p.updated_at DESC`;
const RECOMMENDATIONS = "SELECT r.*,s.company_id,s.branch_id FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY r.updated_at DESC";
const RESOURCES = "SELECT * FROM production_resources WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name";

const generated = [
  planningScopedBranchList('p.*,s.name specification_name', `planning_scenarios p ${SPEC}`, 'p.company_id=?', 'p.branch_id=?', 'p.updated_at DESC'),
  planningScopedBranchList('r.*,s.company_id,s.branch_id', 'planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id', 's.company_id=?', 's.branch_id=?', 'r.updated_at DESC'),
  planningScopedBranchList('*', 'production_resources', 'company_id=?', 'branch_id=?', 'name'),
];

describe('Planning read-tail r8 parameterized branch-scoped list family', () => {
  test('list builder reproduces all three originals byte-identically', () => {
    expect(generated).toEqual([SCENARIOS, RECOMMENDATIONS, RESOURCES]);
  });

  test('every former list site composes the builder and keeps its own prepare/bind', () => {
    const text = src('production-command-sources-read.ts');
    expect(text.match(/planningScopedBranchList\(/g) ?? []).toHaveLength(3);
    expect(text).toContain('planning-scoped-list-sql');
    expect(text).toContain('.bind(companyId, branchId, branchId2)');
    expect(text).toContain('planningCommandExistsGuard('); // r7 family untouched
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-scoped-list-sql.ts');
    expect(selectCount(shared)).toBe(0); // r25: scoped branch-list literal folded into the single-line projection/FROM builder
    for (const sql of generated) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r4-r7 helpers, native read-source consumers and all lock statements stay untouched', () => {
    const guard = src('planning-guard-sql.ts');
    expect(guard).toContain('export const productionResourcesExists =');
    expect(guard).toContain('export const planningAllocatedMinutes =');
    expect(guard).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('resources-read.ts')).toContain('calendarVersionGuard');
    expect(src('scenarios.ts')).toContain("from './production-command-sources-read'");
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('production-command-sources-read.ts')).toContain('NATIVE_PLANNING_REQUIRED');
  });

  test('retirement:ts-statements stays strictly below the r8 BEFORE=35 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(35);
    expect(total).toBeLessThanOrEqual(33);
  });
});
