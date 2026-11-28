import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { productionResourcesExists, planningAllocatedMinutes } from '../../../modules/planning/server/src/adapters/planning-guard-sql';
import { planningCommandExistsGuard } from '../../../modules/planning/server/src/adapters/planning-command-guard-sql';

const srcRoot = resolve(__dirname, '../../../modules/planning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe literals from the r19 BEFORE=19 source. Byte-identity
// oracles only, never live query sources: each generated guard must equal its
// oracle exactly after folding the production-resources EXISTS guard into the
// generalized r7 builder.
const oracle = [
  'EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND branch_id=? AND version=?)',
  "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND version=? AND (?::text IS NULL OR branch_id=?))",
  "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND version=? AND status='active')",
  "EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?))",
];
const predicates = [
  'branch_id=? AND version=?',
  "status='active' AND version=? AND (?::text IS NULL OR branch_id=?)",
  'version=? AND status=\'active\'',
  "status='active' AND (?::text IS NULL OR branch_id=?)",
];
const generated = predicates.map(productionResourcesExists);

describe('Planning read-tail r20 production-resources EXISTS head dedupe', () => {
  test('thin wrapper reproduces every pre-dedupe guard byte-identically', () => {
    expect(generated).toEqual(oracle);
    generated.forEach((sql, i) => {
      expect(sql).toBe(planningCommandExistsGuard('production_resources', `id=? AND company_id=? AND ${predicates[i]}`));
      expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    });
  });

  test('wrapper and generalized builder each keep a single read literal', () => {
    const guard = src('planning-guard-sql.ts');
    expect(selectCount(guard)).toBe(0); // r31: COALESCE(sum) head folded into the fact builder
    expect(guard).toContain('planningCommandExistsGuard("production_resources"');
    expect(guard.match(/export const productionResourcesExists =/g) ?? []).toHaveLength(1);
    expect(guard.match(/export const planningAllocatedMinutes =/g) ?? []).toHaveLength(1);
    expect(selectCount(src('planning-command-guard-sql.ts'))).toBe(0); // r30: EXISTS head folded into the fact builder
    expect(planningAllocatedMinutes).toBe('COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)');
  });

  test('r1-r19 helpers, locks and native reads stay untouched', () => {
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-command-guard-sql.ts')).toContain('planningCommandExistsGuard');
    expect(src('planning-attachment-target-sql.ts')).toContain('planningAttachmentTargetVersionExists');
    expect(readdirSync(resolve(srcRoot, 'adapters'))).not.toContain('planning-select-from-sql.ts');
    expect(existsSync(resolve(srcRoot, 'adapters/planning-select-columns-sql.ts'))).toBe(false);
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts']) {
      expect(src(name)).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
      expect(src(name)).toContain('productionResourcesExists(');
    }
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningCommandExistsGuard');
        expect(text).not.toContain('productionResourcesExists');
      }
    }
  });

  test('retirement:ts-statements strictly lowers the r19 BEFORE=19 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r22 folded the resource-allocation point lookup into the package fact builder (17 -> 16).
    // r23 folded the resource-calendar point lookup into the single-line projection/FROM builder (16 -> 15).
    // r24 folded the scoped single-row family into that same builder (15 -> 14).
    // r25 folded the scoped branch-list family into the single-line projection/FROM builder (14 -> 13).
    expect(total).toBe(5);
    expect(total).toBeLessThan(19);
  });
});
