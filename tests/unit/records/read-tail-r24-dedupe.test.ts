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

// Pre-dedupe rendered SQL copied from the working tree at claim. Whole-string
// byte identity is the oracle: no table, predicate, quote or token may drift.
const pre = {
  approvalSteps: {
    args: ['approval_steps', 's', 'instance_id', 'i'] as const,
    sql: '(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM approval_steps s WHERE s.instance_id=i.id)',
    sha: 'a903b47988996c66fc676e89b01febe5c055c9455df68a56807eed3d091a76c1',
  },
  approvalDecisions: {
    args: ['approval_decisions', 'd', 'instance_id', 'i'] as const,
    sql: '(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM approval_decisions d WHERE d.instance_id=i.id)',
    sha: 'd37fbe33fc25cfb6014b6a6dd704847370aed6c51e07f8bb48c041e10248987c',
  },
  children: {
    args: ['employee_requests', 'child', 'parent_request_id', 'r', 'company_id'] as const,
    sql: '(SELECT jsonb_agg(to_jsonb(child) ORDER BY child.id) FROM employee_requests child WHERE child.parent_request_id=r.id AND child.company_id=r.company_id)',
    sha: '21f738daccdd0950b12ab3ae1d1a9e0b53de4f9688e41f5a9c307d0cf505bd28',
  },
} as const;

// Whole source-image read at claim; the children aggregate was formerly inlined
// and the two approval aggregates came from the narrower approvalChildAgg.
const imageOracle = '563da7850c2613ca153973a90f28dc92bd5473e87e1f07c55a7d6a0220fdeb98';

describe('Records read-tail r24 childRowAgg family', () => {
  test.each([
    ['approvalSteps', pre.approvalSteps],
    ['approvalDecisions', pre.approvalDecisions],
    ['children', pre.children],
  ] as const)('%s renders byte-identically and stays read-only', (_name, member) => {
    const [t, a, fk, parent, company] = member.args as readonly [string, string, string, string, string?];
    const actual = childRowAgg(t, a, fk, parent, company);
    expect(actual).toBe(member.sql);
    expect(sha(actual)).toBe(member.sha);
    expect(sha(member.sql)).toBe(member.sha);
    readOnly(actual);
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

  test('the family is one generator literal with three call sites and no inlined children read', () => {
    expect(shared.match(/export const childRowAgg\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT jsonb_agg\(to_jsonb\(\$\{alias\}/g) ?? []).length).toBe(1);
    expect(shared.match(/childRowAgg\(/g) ?? []).toHaveLength(3); // three call sites
    expect(shared).not.toContain(pre.children.sql);
    expect(shared).not.toMatch(/approvalChildAgg/);
    expect(shared).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(shared).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE/i);
  });

  test('r11-r21 families and all Records source locks stay independent', () => {
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
  });
});

describe('Records read-tail r24 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r24 BEFORE=28 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r24 folds the request children aggregate into the approval child-row
    // generator (28 -> 27); r25 folds the two projection-source single-row
    // guards into one generator (27 -> 26); r26 folds the workspace version
    // fence into the exists guard family (26 -> 25); r27 folds the search
    // references SELECT into the record context detail generator (25 -> 24);
    // r28 folds the reference-edge family into that generator (24 -> 23).
    // No other family changed.
    expect(total).toBe(23);
    expect(total).toBeLessThan(28);
  });
});
