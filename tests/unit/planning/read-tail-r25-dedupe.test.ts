import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningScopedBranchList } from '../../../packages/server-planning/src/adapters/planning-scoped-list-sql';
import { planningSelectColumnsFrom } from '../../../packages/server-planning/src/adapters/planning-select-columns-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Verbatim pre-r25 renderings from HEAD, evaluated with a fixed scenario-spec
// fragment. Byte-identity oracles only, never live query sources: the thin
// wrapper must emit these exactly after folding into the generalized
// single-line projection/FROM builder.
const SPEC = '<SCENARIO_SPEC>';
const SCENARIOS = `SELECT p.*,s.name specification_name FROM planning_scenarios p ${SPEC} WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY p.updated_at DESC`;
const RECOMMENDATIONS = 'SELECT r.*,s.company_id,s.branch_id FROM planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY r.updated_at DESC';
const RESOURCES = 'SELECT * FROM production_resources WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name';

const calls = [
  { columns: 'p.*,s.name specification_name', from: `planning_scenarios p ${SPEC}`, scope: 'p.company_id=?', branch: 'p.branch_id=?', order: 'p.updated_at DESC' },
  { columns: 'r.*,s.company_id,s.branch_id', from: 'planning_recommendations r JOIN planning_scenarios s ON s.id=r.scenario_id', scope: 's.company_id=?', branch: 's.branch_id=?', order: 'r.updated_at DESC' },
  { columns: '*', from: 'production_resources', scope: 'company_id=?', branch: 'branch_id=?', order: 'name' },
];
const generated = calls.map(c => planningScopedBranchList(c.columns, c.from, c.scope, c.branch, c.order));

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Planning read-tail r25 scoped branch-list head fold', () => {
  test('thin wrapper reproduces all three pre-r25 statements byte-identically', () => {
    expect(generated).toEqual([SCENARIOS, RECOMMENDATIONS, RESOURCES]);
    expect(generated.map(sha)).toEqual([
      'aadd818cfa34f8eddcf228188fafdefd22cf666294aceced7a240b37f4fb4350',
      '8c01e3dd72683f6223667979e84ac6667ea45ee240f91ba79e42c1a4584e6c39',
      'ddcb0ed5e97a63192f17cbc59f239e14aa1bce666655fa57c6c314ef39ae49f5',
    ]);
    calls.forEach((c, i) => {
      expect(generated[i]).toBe(planningSelectColumnsFrom(c.columns, `${c.from} WHERE ${c.scope} AND (?::text IS NULL OR ${c.branch}) ORDER BY ${c.order}`));
    });
    generated.forEach(readOnly);
  });

  test('wrapper delegates and the generalized builder owns the single read literal', () => {
    const wrapper = src('planning-scoped-list-sql.ts');
    expect(selectCount(wrapper)).toBe(0);
    expect(wrapper).toContain('planningSelectColumnsFrom(columns, `${from} WHERE ${scope} AND (?::text IS NULL OR ${branch}) ORDER BY ${order}`)');
    expect(wrapper).not.toContain('SELECT ');
    expect(wrapper).toContain('planning-select-columns-sql');
    expect(wrapper.match(/export const planningScopedBranchList =/g) ?? []).toHaveLength(1);
    const head = src('planning-select-columns-sql.ts');
    expect(selectCount(head)).toBe(1);
    expect(head).toContain('`SELECT ${columns} FROM ${rest}`');
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('all three callers are native-owned with unchanged scope arguments', () => {
    const sources = src('production-command-sources-read.ts');
    expect(sources.match(/planningScopedBranchList\(/g) ?? []).toHaveLength(0);
    for (const kind of ['nativeScenarios', 'nativeRecommendations', 'nativeResources'])
      expect(sources).toContain(`${kind}(companyId,branchId,branchId2)`);
    expect(sources).not.toContain(SCENARIOS);
    expect(sources).not.toContain(RECOMMENDATIONS);
    expect(sources).not.toContain(RESOURCES);
  });

  test('r1-r24 helpers, the single lock statement and native readers stay untouched', () => {
    expect(src('planning-select-columns-sql.ts')).toContain('export const planningSelectColumnsFrom =');
    expect(src('planning-select-from-sql.ts')).toContain('export const planningSelectFrom =');
    expect(src('planning-package-fact-sql.ts')).toContain('export const planningPackageFactRows =');
    expect(existsSync(resolve(srcRoot, 'adapters/planning-scoped-row-sql.ts'))).toBe(false);
    expect(src('planning-scoped-list-sql.ts')).toContain('export const planningScopedBranchList =');
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-command-guard-sql.ts')).toContain('export const planningCommandExistsGuard =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetRow =');
    expect(src('planning-attachment-target-sql.ts')).toContain('export const planningAttachmentTargetVersionExists =');
    expect(src('planning-image-source-sql.ts')).toContain('export const planningSourceRows =');
    expect(src('planning-jsonb-agg-sql.ts')).toContain('export const planningJsonbAggOrdered =');
    expect(src('planning-jsonb-row-sql.ts')).toContain('export const planningJsonbRowByKey =');
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
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts', 'capacity.ts', 'planning-scoped-list-sql.ts']) {
      expect(src(name)).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    }
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningScopedBranchList');
        expect(text).not.toContain('planningSelectColumnsFrom');
      }
    }
  });

  test('retirement:ts-statements strictly lowers the r24 BEFORE=14 baseline to 13', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r24 folded the scoped single-row family into the single-line builder (15 -> 14).
    // r25 folded the scoped branch-list family into that same builder (14 -> 13).
    expect(total).toBe(13);
    expect(total).toBeLessThan(14);
  });
});
