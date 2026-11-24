import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { projectionSourceGuardSql } from '../../../packages/server-records/src/adapters/read-tail-projection-source-guard';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/read-tail-projection-source-guard.ts'), 'utf8');
const policy = readFileSync(resolve(srcRoot, 'adapters/source-record-policy.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered SQL copied from the claim source, with the two imported
// subquery values replaced by fixed sentinels. Whole-string byte identity is
// the oracle: no table, predicate, quote or token may drift.
const SRC = '<PROJECTION_SOURCE>';
const READABLE = '<READABLE_SQL>';
const pre = {
  taskGrant: {
    args: [SRC, 'assignments', `id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')`] as const,
    sql: `SELECT 1 FROM (${SRC}) assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')`,
    sha: '56b5a7725dad75f2332cacc5671b0d0527dbef8d590de393ad16d3ec3381627f',
  },
  pmsGuard: {
    args: [SRC, 's', `s.id=? AND s.company_id=? AND s.version=? AND ${READABLE}`] as const,
    sql: `EXISTS(SELECT 1 FROM (${SRC}) s WHERE s.id=? AND s.company_id=? AND s.version=? AND ${READABLE})`,
    sha: '90d3cb823fa3807a8e770c6f2f547213da10e5a090ddc4710a4e5fa28dc45290',
  },
} as const;

describe('Records read-tail r25 projectionSourceGuardSql family', () => {
  test('both members render byte-identically and stay read-only', () => {
    const task = projectionSourceGuardSql(...pre.taskGrant.args);
    expect(task).toBe(pre.taskGrant.sql);
    expect(sha(task)).toBe(pre.taskGrant.sha);
    expect(sha(pre.taskGrant.sql)).toBe(pre.taskGrant.sha);
    readOnly(task);
    const pms = `EXISTS(${projectionSourceGuardSql(...pre.pmsGuard.args)})`;
    expect(pms).toBe(pre.pmsGuard.sql);
    expect(sha(pms)).toBe(pre.pmsGuard.sha);
    expect(sha(pre.pmsGuard.sql)).toBe(pre.pmsGuard.sha);
    readOnly(pms);
  });

  test('the family is one generator literal with two call sites and no inlined projection guard', () => {
    expect(shared.match(/export const projectionSourceGuardSql\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT /g) ?? []).length).toBe(1);
    expect(policy.match(/projectionSourceGuardSql\(/g) ?? []).toHaveLength(2);
    expect(policy).not.toMatch(/SELECT /);
    expect(policy).toContain('projectionSourceGuardSql(identity_accessProjectionSources.assignments');
    expect(policy).toContain('EXISTS(${projectionSourceGuardSql(pmsSourcesSql');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r11-r24 families and all Records source locks stay independent', () => {
    const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).toContain('const referenceEdgesSql ='); // r14
    expect(records).toContain('const recordContextDetailSql ='); // r15
    const ai = readFileSync(resolve(srcRoot, 'adapters/ai-usage-projection.ts'), 'utf8');
    expect(ai).toContain('export const requestAiUsageProjection ='); // r16
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-sql.ts'), 'utf8')).toContain('export const ownerVersionExistsSql ='); // r17
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-count-sql.ts'), 'utf8')).toContain('export const requestAiUsageCountSql ='); // r18
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-exists-sql.ts'), 'utf8')).toContain('export const existsGuardSql ='); // r19
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-select-all.ts'), 'utf8')).toContain('export const selectAllWhereSql ='); // r20
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-employee-request-columns.ts'), 'utf8')).toContain('export const employeeRequestColumnsSql ='); // r21
    const legal = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(legal).toContain('export const childRowAgg ='); // r24
    expect(legal).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(legal).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Records read-tail r25 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the r25 BEFORE=27 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r25 folds the two subquery-source SELECT 1 guards into one generator
    // (27 -> 26); r26 folds the workspace version fence into the exists guard
    // family (26 -> 25). No other family, lock or native seam changed.
    expect(total).toBe(25);
    expect(total).toBeLessThan(27);
  });
});
