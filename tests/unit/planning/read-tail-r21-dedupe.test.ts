import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { planningAttachmentTargetVersionExists } from '../../../packages/server-planning/src/adapters/planning-attachment-target-sql';
import { planningCommandExistsGuard } from '../../../packages/server-planning/src/adapters/planning-command-guard-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-r21 renderings from HEAD, evaluated for the planning_scenario
// target. Byte-identity oracles only, never live query sources: the thin
// wrapper must emit these exactly after folding into the generalized guard.
const VERSION_NO_EXTRA = 'EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=?)';
const VERSION_WITH_EXTRA = 'EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=? AND branch_id=?)';

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Planning read-tail r21 attachment-target version guard dedupe', () => {
  test('thin wrapper reproduces both pre-r21 guards byte-identically', () => {
    const noExtra = planningAttachmentTargetVersionExists('planning_scenarios');
    const withExtra = planningAttachmentTargetVersionExists('planning_scenarios', 'branch_id=?');
    expect(noExtra).toBe(VERSION_NO_EXTRA);
    expect(withExtra).toBe(VERSION_WITH_EXTRA);
    [noExtra, withExtra].forEach(readOnly);
    expect(noExtra).toBe(planningCommandExistsGuard('planning_scenarios', 'id=? AND company_id=? AND version=?'));
    expect(withExtra).toBe(planningCommandExistsGuard('planning_scenarios', 'id=? AND company_id=? AND version=? AND branch_id=?'));
  });

  test('wrapper delegates and the generalized builder owns the single read literal', () => {
    const shared = src('planning-attachment-target-sql.ts');
    expect(selectCount(shared)).toBe(1); // only the target-row builder keeps a SELECT literal
    expect(shared).toContain('planningCommandExistsGuard(table,');
    expect(shared).not.toContain('SELECT 1 FROM');
    expect(shared).not.toContain('EXISTS(');
    expect(shared.match(/export const planningAttachmentTargetVersionExists =/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const planningAttachmentTargetRow =');
    const guard = src('planning-command-guard-sql.ts');
    expect(selectCount(guard)).toBe(1);
    expect(guard).toContain('`EXISTS(SELECT 1 FROM ${table} WHERE ${predicate})`');
  });

  test('both callers keep composing the wrapper with unchanged binds', () => {
    const op = src('attachments-operational-facts.ts');
    expect(op).toContain('planningAttachmentTargetVersionExists(table)');
    expect(op).toContain('[targetId, companyId, version]');
    expect(op).not.toContain('SELECT ');
    const service = src('attachments-service-facts.ts');
    expect(service).toContain('planningAttachmentTargetVersionExists(item.table,item.extra)');
    expect(service).toContain('[targetId, companyId, version]');
    expect(service).not.toContain('SELECT ');
  });

  test('r1-r20 helpers, the single lock statement and native readers stay untouched', () => {
    expect(src('planning-guard-sql.ts')).toContain('export const productionResourcesExists =');
    expect(src('planning-guard-sql.ts')).toContain('export const planningAllocatedMinutes =');
    expect(src('planning-select-from-sql.ts')).toContain('export const planningSelectFrom =');
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
    for (const name of ['resources-read.ts', 'production-command-sources-read.ts']) {
      const text = src(name);
      expect(text).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
      expect(text).toContain('productionResourcesExists(');
    }
    for (const entry of readdirSync(resolve(srcRoot, 'adapters'))) {
      if (entry.endsWith('.rs')) {
        const text = readFileSync(resolve(srcRoot, 'adapters', entry), 'utf8');
        expect(text).not.toContain('planningCommandExistsGuard');
        expect(text).not.toContain('planningAttachmentTargetVersionExists');
      }
    }
  });

  test('retirement:ts-statements strictly lowers the r20 BEFORE=18 baseline', () => {
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
    expect(total).toBe(16);
    expect(total).toBeLessThan(17);
  });
});
