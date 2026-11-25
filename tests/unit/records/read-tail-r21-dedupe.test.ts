import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { employeeRequestColumnsSql } from '../../../packages/server-records/src/adapters/read-tail-employee-request-columns';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/read-tail-employee-request-columns.ts'), 'utf8');
const commerce = readFileSync(resolve(srcRoot, 'adapters/commerce-entrypoint-facts.ts'), 'utf8');
const employeeReads = readFileSync(resolve(srcRoot, 'adapters/employee-requests-read.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered SQL copied from the working tree at claim. Whole-string
// byte identity is the oracle: no column, predicate, quote or token may drift.
const pre = {
  linkedRequest: {
    columns: 'id,title',
    predicate: 'company_id=? AND purchase_order_id=?',
    sql: 'SELECT id,title FROM employee_requests WHERE company_id=? AND purchase_order_id=?',
    sha: '775ab114fdb19239af2513113af63504f310f658e54f128f92fea600f6bd6fb9',
  },
  detailChildren: {
    columns: 'id,title,kind,state',
    predicate: 'parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50',
    sql: 'SELECT id,title,kind,state FROM employee_requests WHERE parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50',
    sha: 'fb582b0beb8167716a4bf81d7686ee664b72d33e4c62a7c480693d07be912f60',
  },
} as const;

describe('Records read-tail r21 employeeRequestColumnsSql family', () => {
  test.each([
    ['linkedRequest', pre.linkedRequest],
    ['detailChildren', pre.detailChildren],
  ] as const)('%s renders byte-identically and stays read-only', (_name, member) => {
    const actual = employeeRequestColumnsSql(member.columns, member.predicate);
    expect(actual).toBe(member.sql);
    expect(sha(actual)).toBe(member.sha);
    expect(sha(member.sql)).toBe(member.sha);
    readOnly(actual);
  });

  test('the family is one generator literal with two call sites and no inlined projection read', () => {
    expect(shared.match(/export const employeeRequestColumnsSql\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT /g) ?? []).length).toBe(0); // r32 delegates to the shared column head
    expect(commerce.match(/employeeRequestColumnsSql\(/g) ?? []).toHaveLength(1);
    expect(employeeReads.match(/employeeRequestColumnsSql\(/g) ?? []).toHaveLength(1);
    expect(commerce).not.toContain(pre.linkedRequest.sql);
    expect(employeeReads).not.toContain(pre.detailChildren.sql);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r11-r20 families and all Records source locks stay independent', () => {
    const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).toContain('const referenceEdgesSql ='); // r14
    expect(records).toContain('const recordContextDetailSql ='); // r15
    const ai = readFileSync(resolve(srcRoot, 'adapters/ai-usage-projection.ts'), 'utf8');
    expect(ai).toContain('export const requestAiUsageProjection ='); // r16
    const r17 = readFileSync(resolve(srcRoot, 'adapters/read-tail-sql.ts'), 'utf8');
    expect(r17).toContain('export const ownerVersionExistsSql =');
    expect((r17.match(/SELECT /g) ?? []).length).toBe(0); // r30 delegates to existsGuardSql
    const r18 = readFileSync(resolve(srcRoot, 'adapters/read-tail-count-sql.ts'), 'utf8');
    expect(r18).toContain('export const requestAiUsageCountSql =');
    expect((r18.match(/SELECT /g) ?? []).length).toBe(1);
    const r19 = readFileSync(resolve(srcRoot, 'adapters/read-tail-exists-sql.ts'), 'utf8');
    expect(r19).toContain('export const existsGuardSql =');
    expect((r19.match(/SELECT /g) ?? []).length).toBe(1);
    const r20 = readFileSync(resolve(srcRoot, 'adapters/read-tail-select-all.ts'), 'utf8');
    expect(r20).toContain('export const selectAllWhereSql =');
    expect((r20.match(/SELECT /g) ?? []).length).toBe(1);
    const locks = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(locks).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(locks).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Records read-tail r21 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r21 BEFORE=30 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r21 deduped the employee-request column reads (linked request + detail
    // children) into one shared parameterized generator (30 -> 29); r23 folds
    // the bounded proposal drain into the request-AI projection generator
    // (29 -> 28); r24 folds the request children aggregate into the approval
    // child-row generator (28 -> 27); r25 folds the two projection-source
    // single-row guards into one generator (27 -> 26); r26 folds the shared
    // workspace version fence into the exists guard family (26 -> 25); r27
    // folds the search references SELECT into the record context detail
    // generator (25 -> 24); r28 folds the reference-edge family into that same
    // generator (24 -> 23); r29 folds the request-events aggregate into that
    // child-row generator with an order frame (23 -> 22); r30 folds the version
    // fence, employee_requests head and request_ai head (22 -> 19). No other
    // family, lock or native seam changed.
    expect(total).toBe(11);
    expect(total).toBeLessThan(30);
  });
});
