import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { productionResourcesExists, planningAllocatedMinutes } from '../../../packages/server-planning/src/adapters/planning-guard-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe literals from HEAD. Byte-identity oracles only, never
// live query sources. Each generated guard must equal its oracle exactly.
const oracle = [
  'EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND branch_id=? AND version=?)',
  "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND version=? AND (?::text IS NULL OR branch_id=?))",
  "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND version=? AND status='active')",
  "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?))",
  'COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)<=?',
  'COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)+?<=?',
];
const generated = [
  productionResourcesExists('branch_id=? AND version=?'),
  productionResourcesExists("status='active' AND version=? AND (?::text IS NULL OR branch_id=?)"),
  productionResourcesExists("version=? AND status='active'"),
  productionResourcesExists("status='active' AND (?::text IS NULL OR branch_id=?)"),
  `${planningAllocatedMinutes}<=?`,
  `${planningAllocatedMinutes}+?<=?`,
];

describe('Planning read-tail r5 byte-identical guard family dedupe', () => {
  test('every parameterized guard is byte-identical to its pre-dedupe literal', () => {
    expect(generated).toEqual(oracle);
  });

  test('both inlined guard families are gone from the owner sources', () => {
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts']) {
      const text = src(name);
      for (const literal of oracle) expect(text).not.toContain(literal);
      expect(text).toContain('productionResourcesExists(');
      expect(text).toContain('planningAllocatedMinutes');
      expect(text).toContain('planning-guard-sql');
    }
    const helper = src('planning-guard-sql.ts');
    expect(helper.match(/export const productionResourcesExists =/g) ?? []).toHaveLength(1);
    expect(helper.match(/export const planningAllocatedMinutes =/g) ?? []).toHaveLength(1);
  });

  test('one shared helper owns the remaining SELECT literal and no lock text entered it', () => {
    const helper = src('planning-guard-sql.ts');
    expect(selectCount(helper)).toBe(1);
    expect(helper).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
  });

  test('all lock statements stay untouched with the read-only purchase source locks intact', () => {
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts']) {
      expect(src(name)).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    }
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
  });

  test('retirement:ts-statements stays strictly below the r5 BEFORE=43 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(43);
    expect(total).toBeLessThanOrEqual(39);
  });
});
