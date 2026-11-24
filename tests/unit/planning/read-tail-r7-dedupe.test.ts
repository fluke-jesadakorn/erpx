import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningCommandExistsGuard } from '../../../packages/server-planning/src/adapters/planning-command-guard-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe renderings from HEAD, evaluated for the Planning
// command-source guards. Byte-identity oracles only, never live query sources.
const COMPARISON = 'EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND branch_id=? AND specification_id=? AND version=?)';
const UNRELEASED = "EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=? AND status='calculated')";
const INDEPENDENT = "EXISTS(SELECT 1 FROM planning_recommendations WHERE id=? AND version=? AND status='proposed' AND created_by<>?)";

const generated = [
  planningCommandExistsGuard('planning_scenarios', 'id=? AND company_id=? AND branch_id=? AND specification_id=? AND version=?'),
  planningCommandExistsGuard('planning_scenarios', "id=? AND company_id=? AND version=? AND status='calculated'"),
  planningCommandExistsGuard('planning_recommendations', "id=? AND version=? AND status='proposed' AND created_by<>?"),
];

describe('Planning read-tail r7 parameterized command-source guard family', () => {
  test('guard builder reproduces all three originals byte-identically', () => {
    expect(generated).toEqual([COMPARISON, UNRELEASED, INDEPENDENT]);
  });

  test('every former guard site composes the builder and keeps its own assertion/bind', () => {
    const text = src('production-command-sources-read.ts');
    for (const literal of [COMPARISON, UNRELEASED, INDEPENDENT]) expect(text).not.toContain(literal);
    expect(text.match(/planningCommandExistsGuard\(/g) ?? []).toHaveLength(3); // three guard call sites
    expect(text).toContain('planning-command-guard-sql');
    expect(text).toContain('[comparisonScenarioId, companyId, value2, specificationId, value3]');
    expect(text).toContain('[id, companyId, expectedVersion]');
    expect(text).toContain('[id, expectedVersion, userId]');
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-command-guard-sql.ts');
    expect(selectCount(shared)).toBe(1);
    for (const sql of generated) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r4/r5/r6 helpers and all lock statements stay untouched', () => {
    const guard = src('planning-guard-sql.ts');
    expect(guard).toContain('export const productionResourcesExists =');
    expect(guard).toContain('export const planningAllocatedMinutes =');
    expect(guard).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    expect(src('resources-read.ts')).toContain('calendarVersionGuard');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
  });

  test('retirement:ts-statements stays strictly below the r7 BEFORE=37 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(37);
    expect(total).toBeLessThanOrEqual(35);
  });
});
