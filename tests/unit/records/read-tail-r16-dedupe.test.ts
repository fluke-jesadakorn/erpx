import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  requestAiUsageProjectionSql,
  recordsRequestUsageSql,
  requestAiUsageMonthlyCountSql,
  requestAiProposalProjection,
  requestAiProposalProjectionSql,
} from '../../../packages/server-records/src/adapters/ai-usage-projection';
import { betaManagementRequestSources } from '../../../packages/server-records/src/adapters/beta-management-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/ai-usage-projection.ts'), 'utf8');
const beta = readFileSync(resolve(srcRoot, 'adapters/beta-management-sources.ts'), 'utf8');
const requestAi = readFileSync(resolve(srcRoot, 'adapters/request-ai.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe SHA-256 of each full literal copied from HEAD. Whole-string byte
// identity is the oracle: no projection, table, predicate or token may drift.
const pre = {
  full: '0e3591004b366c0f13a3ca0bf6287d2ca6b398976ffad150bfc8e731f550270d',
  monthly: '6fd832a2d4a78d25870f26bf28312d8176515622eab20e9708b1b3098800c8db',
  beta: '29bf707dc3f062586453362766c44e40bfc4c13b8ab82352d714c39db11224a2',
  count: '3558f43a6369e72cc812b08c6c7691cbef7b8e916b1ba49de08bdb45f61122b4',
  proposal: '7129a648fc24e19a563a2be2a243b71d846e0ad33cd0c9a534b842c49b02d971',
  proposalDrain: '52fe49c64600a1319632b6a714b15e34dd2cfc42cb87a0dd692417418cfc1505',
} as const;

// Pre-dedupe tail of the bounded drain claim, copied byte-for-byte from HEAD.
const proposalDrainTail = " WHERE (state='queued' OR (state='running' AND claim_until<?)) AND attempts<3 AND (?::text IS NULL OR request_id=?) ORDER BY created_at LIMIT 2";

describe('Records read-tail r16 request_ai_usage projection family', () => {
  test('every family member renders byte-identically and stays read-only', () => {
    expect(sha(requestAiUsageProjectionSql)).toBe(pre.full);
    expect(sha(recordsRequestUsageSql)).toBe(pre.monthly);
    expect(sha(betaManagementRequestSources({} as never).aiUsageSource)).toBe(pre.beta);
    expect(sha(requestAiUsageMonthlyCountSql)).toBe(pre.count);
    expect(sha(requestAiProposalProjectionSql)).toBe(pre.proposal);
    expect(sha(requestAiProposalProjection('id', proposalDrainTail))).toBe(pre.proposalDrain);
    for (const sql of [requestAiUsageProjectionSql, recordsRequestUsageSql, requestAiUsageMonthlyCountSql, requestAiProposalProjectionSql, requestAiProposalProjection('id', proposalDrainTail)]) readOnly(sql);
  });

  test('the request-AI families are single generator literals with no inlined projection', () => {
    expect(shared.split('`SELECT ${columns} FROM ${table}${tail}`')).toHaveLength(1); // r32 delegates to the shared head
    expect(shared.match(/requestAiUsageProjection\(/g) ?? []).toHaveLength(2);
    expect(beta.match(/requestAiUsageProjection\(/g) ?? []).toHaveLength(1);
    expect(shared).toContain("requestAiProjectionHead('request_ai_usage', columns)");
    expect(shared).toContain("requestAiProjectionHead('request_ai_proposals', columns, tail)");
    expect(shared.match(/requestAiProposalProjection\(/g) ?? []).toHaveLength(1);
    expect(requestAi.match(/requestAiProposalProjection\(/g) ?? []).toHaveLength(1);
    expect(requestAi).not.toContain('SELECT id FROM request_ai_proposals WHERE');
    expect(selectCount(shared)).toBe(0); // monthly count now delegates to the shared r32 head
    expect(shared).toContain("projectionColumnHeadSql(\n  'count(*)::int count',");
    expect(selectCount(readFileSync(resolve(srcRoot, 'adapters/read-tail-r32-sql.ts'), 'utf8'))).toBe(1);
    expect(selectCount(requestAi)).toBe(0);
    expect(selectCount(beta)).toBe(0);
  });

  test('r11-r15 families and all Records source locks stay independent', () => {
    const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
    expect(records).toContain('const recordVersionExistsSql ='); // r11
    expect(records).toContain('const recordRowSelectSql ='); // r12
    expect(records).not.toContain('const referenceEdgesSql ='); // r14 retired
    expect(records).not.toContain('const recordContextDetailSql ='); // r15 retired
    const locks = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(locks).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(locks).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
    expect(shared).not.toMatch(/LOCK\s+TABLE/i);
    expect(beta).not.toMatch(/LOCK\s+TABLE/i);
  });
});

describe('Records read-tail r16 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r16 BEFORE=37 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(37);
  });
});
