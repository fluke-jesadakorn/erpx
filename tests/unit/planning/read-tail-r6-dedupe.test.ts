import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  planningAttachmentTargetRow,
  planningAttachmentTargetVersionExists,
} from '../../../packages/server-planning/src/adapters/planning-attachment-target-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-planning/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Verbatim pre-dedupe renderings from HEAD, evaluated for the planning_scenario
// target. Byte-identity oracles only, never live query sources.
const ROW_OPERATIONAL = 'SELECT company_id,branch_id branch_id,NULL::text department_id,version FROM planning_scenarios WHERE id=? AND company_id=?';
const ROW_POLICY = 'SELECT company_id,branch_id AS branch_id,NULL AS department_id,version AS version FROM planning_scenarios WHERE id=? AND company_id=?';
const VERSION_NO_EXTRA = 'EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=?)';
const VERSION_WITH_EXTRA = 'EXISTS(SELECT 1 FROM planning_scenarios WHERE id=? AND company_id=? AND version=? AND branch_id=?)';

const readOnly = (sql: string) => {
  expect(sql.toUpperCase()).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Planning read-tail r6 parameterized attachment-target family', () => {
  test('target-row builder reproduces both originals byte-identically', () => {
    expect(planningAttachmentTargetRow('branch_id branch_id,NULL::text department_id,version', 'planning_scenarios')).toBe(ROW_OPERATIONAL);
    expect(planningAttachmentTargetRow('branch_id AS branch_id,NULL AS department_id,version AS version', 'planning_scenarios', '')).toBe(ROW_POLICY);
    [ROW_OPERATIONAL, ROW_POLICY].forEach(readOnly);
  });

  test('target-version guard builder reproduces both originals byte-identically', () => {
    expect(planningAttachmentTargetVersionExists('planning_scenarios')).toBe(VERSION_NO_EXTRA);
    expect(planningAttachmentTargetVersionExists('planning_scenarios', 'branch_id=?')).toBe(VERSION_WITH_EXTRA);
    [VERSION_NO_EXTRA, VERSION_WITH_EXTRA].forEach(readOnly);
  });

  test('every former site composes the builders and keeps its own prepare/bind', () => {
    const op = src('attachments-operational-facts.ts');
    expect(op).toContain('planningAttachmentTargetRow(');
    expect(op).toContain('planningAttachmentTargetVersionExists(');
    expect(op).not.toContain('SELECT ');
    expect(op).toContain('.bind(facts.targetId,facts.companyId).first<TargetRow>()');
    expect(op).toContain('`attachment-owner-${index}`');

    const policy = src('attachments-policy-facts.ts');
    expect(policy).toContain('planningAttachmentTargetRow(');
    expect(policy).not.toContain('SELECT ');
    expect(policy).toContain('.bind(facts.targetId,facts.companyId).first<');

    const service = src('attachments-service-facts.ts');
    expect(service).toContain('planningAttachmentTargetVersionExists(');
    expect(service).not.toContain('SELECT ');
    expect(service).toContain('[targetId, companyId, version]');
  });

  test('r4/r5 helpers and all lock statements stay untouched', () => {
    const guard = src('planning-guard-sql.ts');
    expect(guard).toContain('export const productionResourcesExists =');
    expect(guard).toContain('export const planningAllocatedMinutes =');
    expect(guard).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
    expect(src('resources-read.ts')).toContain('calendarVersionGuard');
    expect(src('purchase-sources.ts')).toContain('LOCK TABLE planning_scenarios,planning_recommendations,planning_scenario_events,planning_resource_allocations,production_resources,production_resource_calendar IN ${writer?"SHARE ROW EXCLUSIVE":"SHARE"} MODE');
  });

  test('shared module declares each read literal exactly once', () => {
    const shared = src('planning-attachment-target-sql.ts');
    expect(selectCount(shared)).toBe(1); // r21: version EXISTS literal folded into the generalized guard
  });

  test('retirement:ts-statements stays strictly below the r6 BEFORE=39 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(39);
    expect(total).toBeLessThanOrEqual(37);
  });
});
