import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { childRowAgg, legalRequestAttachmentSourceImage } from '../../../packages/server-records/src/adapters/legal-attachment-read-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered events aggregate copied from the inline literal at claim.
// Whole-string byte identity is the oracle: alias, order, table, fk, parent and
// company scope must not drift.
const pre = {
  events: {
    args: ['employee_request_events', 'e', 'request_id', 'r', 'company_id', 'e.created_at,e.id'] as const,
    sql: '(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.created_at,e.id) FROM employee_request_events e WHERE e.request_id=r.id AND e.company_id=r.company_id)',
    sha: '710bde4365b1823da8f5b223c4389a0d3082f598428125a46c716c37cfc501fe',
  },
} as const;

// Whole source-image read at claim; the image is unchanged by the fold.
const imageOracle = '563da7850c2613ca153973a90f28dc92bd5473e87e1f07c55a7d6a0220fdeb98';

describe('Records read-tail r29 request-events aggregate fold', () => {
  test('the events aggregate renders byte-identically and stays read-only', () => {
    const [t, a, fk, parent, company, order] = pre.events.args;
    const actual = childRowAgg(t, a, fk, parent, company, order);
    expect(actual).toBe(pre.events.sql);
    expect(sha(actual)).toBe(pre.events.sha);
    expect(sha(pre.events.sql)).toBe(pre.events.sha);
    readOnly(actual);
  });

  test('default order preserves the r24 approval and children aggregates', () => {
    expect(childRowAgg('approval_steps', 's', 'instance_id', 'i')).toBe(
      '(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM approval_steps s WHERE s.instance_id=i.id)',
    );
    expect(childRowAgg('approval_decisions', 'd', 'instance_id', 'i')).toBe(
      '(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM approval_decisions d WHERE d.instance_id=i.id)',
    );
    expect(childRowAgg('employee_requests', 'child', 'parent_request_id', 'r', 'company_id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(child) ORDER BY child.id) FROM employee_requests child WHERE child.parent_request_id=r.id AND child.company_id=r.company_id)',
    );
  });

  test('the whole request attachment image SQL is unchanged', async () => {
    let captured = '';
    const db = {
      prepare(sql: string) {
        captured = sql;
        return { bind() { return this; }, first() { return Promise.resolve(null); } };
      },
    };
    await legalRequestAttachmentSourceImage(db as never, 'c1', 'r1');
    expect(sha(captured)).toBe(imageOracle);
    expect((captured.match(/SELECT /g) ?? []).length).toBe(6);
  });

  test('the family is one generator literal with four call sites and no inlined events read', () => {
    expect(shared.match(/export const childRowAgg\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT jsonb_agg\(to_jsonb\(\$\{alias\}/g) ?? []).length).toBe(1);
    expect(shared.match(/childRowAgg\(/g) ?? []).toHaveLength(4);
    expect(shared).toContain("childRowAgg('employee_request_events','e','request_id','r','company_id','e.created_at,e.id')");
    expect(shared).not.toContain(pre.events.sql);
    expect(shared).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(shared).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  test('r11-r28 families and all Records source locks stay independent', () => {
    const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).toContain('const referenceEdgesSql ='); // r14
    expect(records).toContain('const recordContextDetailSql ='); // r15
    expect(records).toContain('export const recordContextDetailSql ='); // r27/r28
    const ai = readFileSync(resolve(srcRoot, 'adapters/ai-usage-projection.ts'), 'utf8');
    expect(ai).toContain('export const requestAiUsageProjection ='); // r16
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-sql.ts'), 'utf8')).toContain('export const ownerVersionExistsSql ='); // r17
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-count-sql.ts'), 'utf8')).toContain('export const requestAiUsageCountSql ='); // r18
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-exists-sql.ts'), 'utf8')).toContain('export const existsGuardSql ='); // r19
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-select-all.ts'), 'utf8')).toContain('export const selectAllWhereSql ='); // r20
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-employee-request-columns.ts'), 'utf8')).toContain('export const employeeRequestColumnsSql ='); // r21
    expect(readFileSync(resolve(srcRoot, 'adapters/read-tail-projection-source-guard.ts'), 'utf8')).toContain('export const projectionSourceGuardSql ='); // r25
  });
});

describe('Records read-tail r29 retirement check', () => {
  test('retirement:ts-statements folds the events aggregate (23 -> 22) and r30 heads (22 -> 19)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r28 left the request-events aggregate inline (24 -> 23). r29 admits it
    // to the childRowAgg family through an optional order frame (23 -> 22). r30
    // folds the version fence, employee_requests head and request_ai head into
    // shared generators (22 -> 19). No rendered SQL, bind, lock or native seam
    // changes.
    expect(total).toBe(12);
    expect(total).toBeLessThan(23);
  });
});
