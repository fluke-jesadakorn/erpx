import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningPackageFactRows } from '../../../packages/server-planning/src/adapters/planning-package-fact-sql';
import { planningResourceAllocationRows } from '../../../packages/server-planning/src/adapters/planning-resource-allocation-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-r22 renderings from HEAD, evaluated for the two production
// resource-allocation sites. Byte-identity oracles only, never live query
// sources: the thin wrapper must emit these exactly after folding into the
// generalized package fact builder.
const CAPACITY_MINUTES = 'SELECT COALESCE(sum(minutes),0) minutes FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL';
const ACTIVE_ALLOCATION_GUARD = 'NOT EXISTS(SELECT 1 FROM planning_resource_allocations WHERE resource_id=? AND released_at IS NULL)';

const generated = [
  planningResourceAllocationRows('COALESCE(sum(minutes),0) minutes', 'resource_id=? AND work_date=? AND released_at IS NULL'),
  `NOT EXISTS(${planningResourceAllocationRows('1', 'resource_id=? AND released_at IS NULL')})`,
];

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Planning read-tail r22 resource-allocation point-lookup fold', () => {
  test('thin wrapper reproduces both pre-r22 statements byte-identically', () => {
    expect(generated).toEqual([CAPACITY_MINUTES, ACTIVE_ALLOCATION_GUARD]);
    expect(generated[0]).toBe(planningPackageFactRows('COALESCE(sum(minutes),0) minutes', 'planning_resource_allocations', 'resource_id=? AND work_date=? AND released_at IS NULL'));
    expect(generated[1]).toBe(`NOT EXISTS(${planningPackageFactRows('1', 'planning_resource_allocations', 'resource_id=? AND released_at IS NULL')})`);
    generated.forEach(readOnly);
  });

  test('wrapper delegates and the generalized builder owns the single read literal', () => {
    const shared = src('planning-resource-allocation-sql.ts');
    expect(selectCount(shared)).toBe(0);
    expect(shared).toContain("planningPackageFactRows(projection, 'planning_resource_allocations', predicate)");
    expect(shared).not.toContain('SELECT ');
    expect(shared.match(/export const planningResourceAllocationRows =/g) ?? []).toHaveLength(1);
    const fact = src('planning-package-fact-sql.ts');
    expect(selectCount(fact)).toBe(1);
    expect(fact).toContain('`SELECT ${projection} FROM ${source} WHERE ${predicate}`');
    expect(fact).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('both callers keep composing the wrapper with unchanged binds', () => {
    const capacity = src('capacity.ts');
    expect(capacity).toContain("planningResourceAllocationRows('COALESCE(sum(minutes),0) minutes', 'resource_id=? AND work_date=? AND released_at IS NULL')");
    expect(capacity).toContain('.bind(resourceId, date).first<{ minutes: number }>()');
    expect(capacity).toContain('planning-resource-allocation-sql');
    expect(capacity).not.toContain('SELECT ');

    const resources = src('resources-read.ts');
    expect(resources).toContain("planningResourceAllocationRows('1', 'resource_id=? AND released_at IS NULL')");
    expect(resources).toContain('guardNoActiveAllocation');
    expect(resources).toContain('planning-resource-allocation-sql');
    expect(resources).not.toContain('SELECT ');
  });

  test('r1-r21 helpers, the single lock statement and native readers stay untouched', () => {
    expect(src('planning-package-fact-sql.ts')).toContain('export const planningPackageFactRows =');
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('export const planningCommandExistsGuard =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetRow =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetVersionExists =');
    expect(readdirSync(resolve(srcRoot, 'adapters'))).not.toContain('planning-select-from-sql.ts');
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
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts']) {
      expect(src(name)).toContain('productionResourcesExists(');
    }
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningPackageFactRows');
        expect(text).not.toContain('planningResourceAllocationRows');
      }
    }
  });

  test('retirement:ts-statements strictly lowers the r21 BEFORE=17 baseline', () => {
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
    expect(total).toBe(11);
    expect(total).toBeLessThan(17);
  });
});
