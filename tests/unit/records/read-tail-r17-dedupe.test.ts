import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { ownerVersionExistsSql } from '../../../packages/server-records/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/read-tail-sql.ts'), 'utf8');
const employee = readFileSync(resolve(srcRoot, 'adapters/employee-requests.ts'), 'utf8');
const attachments = readFileSync(resolve(srcRoot, 'adapters/attachments-service-facts.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered literal SHAs copied from HEAD. Whole-string byte identity
// is the oracle: no table, predicate, tail or bind may drift.
const pre = {
  employee: {
    tail: ' AND state=? AND evidence_version=?',
    sha: 'c6c6cc7e1b38f078a315c149ebec63c3841b7115104382a2d962e24c9464a74b',
  },
  attachment: {
    tail: '',
    sha: 'f535586f2cb9c008beeaa10a9907968d235228f9262957c39efac354bacf99bd',
  },
} as const;

describe('Records read-tail r17 ownerVersionExistsSql family', () => {
  test('both callers render byte-identically and stay read-only', () => {
    const er = ownerVersionExistsSql('employee_requests', pre.employee.tail);
    const at = ownerVersionExistsSql('employee_requests', pre.attachment.tail);
    expect(sha(er)).toBe(pre.employee.sha);
    expect(sha(at)).toBe(pre.attachment.sha);
    readOnly(er);
    readOnly(at);
  });

  test('the family is one named delegating generator with two callers and no inlined guard', () => {
    expect(shared.match(/export const ownerVersionExistsSql\s*=/g) ?? []).toHaveLength(1);
    expect(shared).toContain("existsGuardSql(table, `id=? AND company_id=? AND version=?${tail}`)");
    expect((shared.match(/SELECT /g) ?? []).length).toBe(0); // r30 delegates to existsGuardSql
    expect(employee.match(/ownerVersionExistsSql\(/g) ?? []).toHaveLength(1);
    expect(attachments.match(/ownerVersionExistsSql\(/g) ?? []).toHaveLength(1);
    expect(employee).not.toContain('EXISTS(SELECT 1 FROM employee_requests WHERE id=?');
    expect(attachments).not.toContain('EXISTS(SELECT 1 FROM ${item.table}');
  });

  test('r11-r16 families and all Records source locks stay independent', () => {
    const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).not.toContain('const referenceEdgesSql ='); // r14 retired
    expect(records).not.toContain('const recordContextDetailSql ='); // r15 retired
    const ai = readFileSync(resolve(srcRoot, 'adapters/ai-usage-projection.ts'), 'utf8');
    expect(ai).toContain('export const requestAiUsageProjection ='); // r16
    const locks = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(locks).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(locks).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
    expect(shared).not.toMatch(/LOCK\s+TABLE/i);
    expect(employee).not.toMatch(/LOCK\s+TABLE|pg_advisory/i);
    expect(attachments).not.toMatch(/LOCK\s+TABLE|pg_advisory/i);
  });
});

describe('Records read-tail r17 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r17 BEFORE=35 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r17 deduped the 2x EXISTS version-fence guard in employee-requests.ts and
    // attachments-service-facts.ts into one shared parameterized fragment.
    // r17's achievement is a ceiling of 34; later rounds (r18+) reduce further.
    expect(total).toBeLessThanOrEqual(34);
    expect(total).toBeLessThan(35);
  });
});
