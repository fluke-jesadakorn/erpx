import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningResourceAllocationRows } from '../../../modules/planning/server/src/adapters/planning-resource-allocation-sql';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe literals from HEAD for the two production resource-allocation
// point lookups keyed by resource_id/released_at. Byte-identity oracles only, never
// live query sources.
const CAPACITY_MINUTES = 'SELECT COALESCE(sum(minutes),0) minutes FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL';
const ACTIVE_ALLOCATION_GUARD = 'NOT EXISTS(SELECT 1 FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL)';

const generated = [
  planningResourceAllocationRows('COALESCE(sum(minutes),0) minutes', 'resource_id=? AND work_date=? AND released_at IS NULL'),
  `NOT EXISTS(${planningResourceAllocationRows('1', 'resource_id=? AND released_at IS NULL')})`,
];

describe('Planning read-tail r11 parameterized resource-allocation point-lookup family', () => {
  test('builder reproduces both originals byte-identically', () => {
    expect(generated).toEqual([CAPACITY_MINUTES, ACTIVE_ALLOCATION_GUARD]);
  });

  test('both former sites compose the builder and keep their own prepare/bind', () => {
    const capacity = src('capacity.ts');
    expect(capacity).not.toContain(CAPACITY_MINUTES);
    expect(capacity).toContain("planningResourceAllocationRows('COALESCE(sum(minutes),0) minutes', 'resource_id=? AND work_date=? AND released_at IS NULL')");
    expect(capacity).toContain('.bind(resourceId, date).first<{ minutes: number }>()');
    expect(capacity).toContain('planning-resource-allocation-sql');

    const resources = src('resources-read.ts');
    expect(resources).not.toContain(ACTIVE_ALLOCATION_GUARD);
    expect(resources).toContain("planningResourceAllocationRows('1', 'resource_id=? AND released_at IS NULL')");
    expect(resources).toContain('guardNoActiveAllocation');
    expect(resources).toContain('planning-resource-allocation-sql');
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-resource-allocation-sql.ts');
    expect(selectCount(shared)).toBe(0); // r22: read literal folded into the package fact builder
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of generated) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r4-r10 helpers, native read consumers and all lock statements stay untouched', () => {
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetRow');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(src('planning-scoped-list-sql.ts')).toContain('planningScopedBranchList');
    expect(existsSync(resolve(srcRoot, 'adapters/planning-scoped-row-sql.ts'))).toBe(false);
    expect(src('planning-calendar-source-sql.ts')).toContain('resourceCalendarByKey');
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('capacity.ts')).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    expect(src('planning-guard-sql.ts')).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    expect(src('resources-read.ts')).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);

    const nativeRead = readFileSync(resolve(srcRoot, 'adapters', 'planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
  });

  test('retirement:ts-statements stays strictly below the r11 BEFORE=31 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(31);
    expect(total).toBeLessThanOrEqual(30);
  });
});
