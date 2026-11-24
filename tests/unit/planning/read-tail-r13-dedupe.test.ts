import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningPackageFactRows } from '../../../packages/server-planning/src/adapters/planning-package-fact-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe UNION-ALL fact branches from HEAD (byte-identity oracles only,
// never live query sources). The three branches differ only in projection, source
// and predicate, so one owner-local builder now emits all three.
const ORIGINAL_BRANCHES = [
  "SELECT 'scenario' kind,s.id,to_jsonb(s)::text fact FROM scenarios s WHERE s.status NOT IN ('released','cancelled') OR NOT s.source_valid",
  "SELECT 'recommendation',r.id,to_jsonb(r)::text FROM planning_recommendations r JOIN scenarios s ON s.id=r.scenario_id WHERE r.status NOT IN ('approved','rejected')",
  "SELECT 'capacity',a.id,to_jsonb(a)::text FROM planning_resource_allocations a JOIN scenarios s ON s.id=a.scenario_id WHERE a.released_at IS NULL",
];
const CALL_EXPRS = [
  `\${planningPackageFactRows("'scenario' kind,s.id,to_jsonb(s)::text fact", 'scenarios s', "s.status NOT IN ('released','cancelled') OR NOT s.source_valid")}`,
  `\${planningPackageFactRows("'recommendation',r.id,to_jsonb(r)::text", 'planning_recommendations r JOIN scenarios s ON s.id=r.scenario_id', "r.status NOT IN ('approved','rejected')")}`,
  `\${planningPackageFactRows("'capacity',a.id,to_jsonb(a)::text", 'planning_resource_allocations a JOIN scenarios s ON s.id=a.scenario_id', 'a.released_at IS NULL')}`,
];

const generated = [
  planningPackageFactRows("'scenario' kind,s.id,to_jsonb(s)::text fact", 'scenarios s', "s.status NOT IN ('released','cancelled') OR NOT s.source_valid"),
  planningPackageFactRows("'recommendation',r.id,to_jsonb(r)::text", 'planning_recommendations r JOIN scenarios s ON s.id=r.scenario_id', "r.status NOT IN ('approved','rejected')"),
  planningPackageFactRows("'capacity',a.id,to_jsonb(a)::text", 'planning_resource_allocations a JOIN scenarios s ON s.id=a.scenario_id', 'a.released_at IS NULL'),
];

describe('Planning read-tail r13 parameterized package-fact branch family', () => {
  test('builder reproduces all three originals byte-identically', () => {
    expect(generated).toEqual(ORIGINAL_BRANCHES);
  });

  test('replacing builder calls restores the verbatim pre-dedupe statement block', () => {
    let restored = src('package-obligations.ts');
    CALL_EXPRS.forEach((call, i) => {
      expect(restored).toContain(call);
      restored = restored.replace(call, ORIGINAL_BRANCHES[i]);
    });
    expect(restored).toContain(` ${ORIGINAL_BRANCHES[0]}
 UNION ALL ${ORIGINAL_BRANCHES[1]}
 UNION ALL ${ORIGINAL_BRANCHES[2]}`);
    const current = src('package-obligations.ts');
    for (const original of ORIGINAL_BRANCHES) expect(current).not.toContain(original);
  });

  test('shared module declares the read literal exactly once and stays read-only', () => {
    const shared = src('planning-package-fact-sql.ts');
    expect(selectCount(shared)).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of generated) {
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('r4-r12 helpers, native consumers and all lock statements stay untouched', () => {
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
    expect(src('executive-metric-projections.ts')).toContain('planningBranchDisplayLabelSql');
    expect(src('executive-metric-projections.ts')).toContain('metricLabelCountProjection');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    expect(src('package-obligations.ts')).toContain('planningPurchaseSourceReadLocks(database)');

    const nativeRead = readFileSync(resolve(srcRoot, 'adapters/planning_read.rs'), 'utf8');
    expect(nativeRead).toContain('pub async fn workspace');
    expect(nativeRead).toContain('pub fn parse_shortages');
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) expect(readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8')).not.toContain('planningPackageFactRows');
    }
  });

  test('retirement:ts-statements stays strictly below the r12 BEFORE=29 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(29);
    expect(total).toBeLessThanOrEqual(28);
  });
});
