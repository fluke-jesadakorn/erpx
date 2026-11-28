import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { selectAllWhereSql } from '../../../modules/records/server/src/adapters/read-tail-select-all';

const srcRoot = resolve(__dirname, '../../../modules/records/server/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/read-tail-select-all.ts'), 'utf8');
const employeeReads = readFileSync(resolve(srcRoot, 'adapters/employee-requests-read.ts'), 'utf8');
const documentSources = readFileSync(resolve(srcRoot, 'adapters/workflow-document-sources.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered SQL copied from HEAD. Whole-string byte identity is the
// oracle: no table, predicate, quote or token may drift. The first member is
// the employee-request row read; the next two are the workflow document detail
// and page reads for the 'request' and 'record' tables.
const pre = {
  requestByRow: {
    table: 'employee_requests',
    predicate: 'id=? AND company_id=?',
    sql: 'SELECT * FROM employee_requests WHERE id=? AND company_id=?',
    sha: '674e1a56dee56828348221ec92c70be361f46d29144f2aad326153215b3cbc42',
  },
  requestDocument: {
    table: 'employee_requests',
    predicate: 'company_id=? AND id',
    sql: 'SELECT * FROM employee_requests WHERE company_id=? AND id',
    sha: '9683c80e5487bd59307309a77d7d227d0e946923fb0b5fea520924de325b5fb6',
  },
  recordDocument: {
    table: 'workspace_records',
    predicate: 'company_id=? AND id',
    sql: 'SELECT * FROM workspace_records WHERE company_id=? AND id',
    sha: '0b57a96c14a79c5e008247d70b8dc178c20bea2c080f964f3bbfd1b889609a31',
  },
} as const;

describe('Records read-tail r20 selectAllWhereSql family', () => {
  test.each([
    ['requestByRow', pre.requestByRow],
    ['requestDocument', pre.requestDocument],
    ['recordDocument', pre.recordDocument],
  ] as const)('%s renders byte-identically and stays read-only', (_name, member) => {
    const actual = selectAllWhereSql(member.table, member.predicate);
    expect(actual).toBe(member.sql);
    expect(sha(actual)).toBe(member.sha);
    expect(sha(member.sql)).toBe(member.sha);
    readOnly(actual);
  });

  test('the family is one generator literal with three call sites and no inlined SELECT-* read', () => {
    expect(shared.match(/export const selectAllWhereSql\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT /g) ?? []).length).toBe(0); // head now lives in r32
    expect(shared).toContain("import { projectionColumnHeadSql } from './read-tail-r32-sql';");
    expect(shared).toContain("projectionColumnHeadSql('*', table, ` WHERE ${predicate}`)");
    expect(employeeReads.match(/selectAllWhereSql\(/g) ?? []).toHaveLength(1);
    expect(documentSources.match(/selectAllWhereSql\(/g) ?? []).toHaveLength(1);
    expect(employeeReads).not.toContain(pre.requestByRow.sql);
    expect(documentSources).not.toContain(pre.requestDocument.sql);
    expect(documentSources).not.toContain(pre.recordDocument.sql);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r11-r19 families and all Records source locks stay independent', () => {
    const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).not.toContain('const referenceEdgesSql ='); // r14 retired
    expect(records).not.toContain('const recordContextDetailSql ='); // r15 retired
    const ai = readFileSync(resolve(srcRoot, 'adapters/ai-usage-projection.ts'), 'utf8');
    expect(ai).toContain('export const requestAiUsageProjection ='); // r16
    const r17 = readFileSync(resolve(srcRoot, 'adapters/read-tail-sql.ts'), 'utf8');
    expect(r17).toContain('export const ownerVersionExistsSql =');
    expect((r17.match(/SELECT /g) ?? []).length).toBe(0); // r30 delegates to existsGuardSql
    const r18 = readFileSync(resolve(srcRoot, 'adapters/read-tail-count-sql.ts'), 'utf8');
    expect(r18).toContain('export const requestAiUsageCountSql =');
    expect((r18.match(/SELECT /g) ?? []).length).toBe(0);
    const r19 = readFileSync(resolve(srcRoot, 'adapters/read-tail-exists-sql.ts'), 'utf8');
    expect(r19).toContain('export const existsGuardSql =');
    expect((r19.match(/SELECT /g) ?? []).length).toBe(1);
    const locks = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(locks).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(locks).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Records read-tail r20 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r20 BEFORE=31 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r20 deduped the two SELECT-* by-key reads (employee row read + workflow
    // document read) into one shared parameterized generator (31 -> 30);
    // r21 further deduped the employee-request column reads (30 -> 29).
    expect(total).toBeLessThanOrEqual(30);
    expect(total).toBeLessThan(31);
  });
});
