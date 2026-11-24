import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { requestAiUsageCountSql } from '../../../packages/server-records/src/adapters/read-tail-count-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/read-tail-count-sql.ts'), 'utf8');
const requestAi = readFileSync(resolve(srcRoot, 'adapters/request-ai.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered subquery SHAs copied from HEAD. Whole-string byte identity
// is the oracle: no table, predicate, quote or token may drift.
const pre = {
  quota: {
    predicate: "company_id=? AND state='reserved' AND lease_until>?",
    sha: '8876dbee4f0260a0b4ccd90aecc87c52f33b8abcb613790979d640989085c779',
  },
  rate: {
    predicate: 'actor_id=? AND created_at>?',
    sha: '7a07baa5c8d9fdfe586b34d4146a6fc4c4b8a17a305383261178083b844642be',
  },
} as const;

describe('Records read-tail r18 requestAiUsageCountSql family', () => {
  test('both callers render byte-identically and stay read-only', () => {
    const quota = requestAiUsageCountSql(pre.quota.predicate);
    const rate = requestAiUsageCountSql(pre.rate.predicate);
    expect(sha(quota)).toBe(pre.quota.sha);
    expect(sha(rate)).toBe(pre.rate.sha);
    readOnly(quota);
    readOnly(rate);
  });

  test('the family is one generator literal with two callers and no inlined count', () => {
    expect(shared.match(/export const requestAiUsageCountSql\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT /g) ?? []).length).toBe(1);
    expect(requestAi.match(/requestAiUsageCountSql\(/g) ?? []).toHaveLength(2);
    expect(requestAi).not.toContain('SELECT count(*) FROM request_ai_usage WHERE company_id=?');
    expect(requestAi).not.toContain('SELECT count(*) FROM request_ai_usage WHERE actor_id=?');
  });

  test('r11-r17 families and all Records source locks stay independent', () => {
    const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).toContain('const referenceEdgesSql ='); // r14
    expect(records).toContain('const recordContextDetailSql ='); // r15
    const ai = readFileSync(resolve(srcRoot, 'adapters/ai-usage-projection.ts'), 'utf8');
    expect(ai).toContain('export const requestAiUsageProjection ='); // r16
    const r17 = readFileSync(resolve(srcRoot, 'adapters/read-tail-sql.ts'), 'utf8');
    expect(r17).toContain('export const ownerVersionExistsSql ='); // r17
    expect((r17.match(/SELECT /g) ?? []).length).toBe(1); // r17 oracle untouched
    const locks = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(locks).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(locks).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
    expect(shared).not.toMatch(/LOCK\s+TABLE/i);
    expect(requestAi).not.toMatch(/LOCK\s+TABLE|pg_advisory/i);
  });
});

describe('Records read-tail r18 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r18 BEFORE=34 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r18 deduped the 2x SELECT count(*) request_ai_usage quota subqueries in
    // request-ai.ts into one shared parameterized fragment (34 -> 33).
    // r18's achievement is a ceiling of 33; later rounds (r19+) reduce further.
    expect(total).toBeLessThanOrEqual(33);
    expect(total).toBeLessThan(34);
  });
});
