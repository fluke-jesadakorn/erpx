import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { existsGuardSql } from '../../../packages/server-records/src/adapters/read-tail-exists-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const shared = readFileSync(resolve(srcRoot, 'adapters/read-tail-exists-sql.ts'), 'utf8');
const records = readFileSync(resolve(srcRoot, 'adapters/records.ts'), 'utf8');
const requestAi = readFileSync(resolve(srcRoot, 'adapters/request-ai.ts'), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe rendered guard SQL copied from HEAD. Whole-string byte identity is
// the oracle: no table, predicate, quote or token may drift.
const pre = {
  recordVersion: {
    table: 'workspace_records',
    predicate: 'module_id=? AND id=? AND version=? AND archived_at IS NULL',
    sql: 'EXISTS(SELECT 1 FROM workspace_records WHERE module_id=? AND id=? AND version=? AND archived_at IS NULL)',
    sha: '2b226270c2e5edc92e9886823472fa5d50a127fb104c1b37b4cf130de184d570',
  },
  companyVersion: {
    table: 'workspace_records',
    predicate: 'company_id=? AND module_id=? AND id=? AND version=? AND archived_at IS NULL',
    sql: 'EXISTS(SELECT 1 FROM workspace_records WHERE company_id=? AND module_id=? AND id=? AND version=? AND archived_at IS NULL)',
    sha: 'd1ec49beaf8c9e49e8152c4c9765589a173fc689fd28c66ec2d54a51f0d62547',
  },
  suggestionReady: {
    table: 'record_field_suggestions',
    predicate: "id=? AND state='ready'",
    sql: "EXISTS(SELECT 1 FROM record_field_suggestions WHERE id=? AND state='ready')",
    sha: 'eeb954020eb938a488a3fc30c3616d135c02faf63c91bf4df4b2fe9eab21a2b3',
  },
  proposalLease: {
    table: 'request_ai_proposals',
    predicate: "id=? AND state='running' AND lease_token=? AND claim_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
    sql: "EXISTS(SELECT 1 FROM request_ai_proposals WHERE id=? AND state='running' AND lease_token=? AND claim_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint))",
    sha: 'fd39ab725503e4cfc74e209034e20fd6fd452db6e51326960ee4a456aa2935b3',
  },
} as const;

describe('Records read-tail r19 existsGuardSql family', () => {
  test.each([
    ['recordVersion', pre.recordVersion],
    ['companyVersion', pre.companyVersion],
    ['suggestionReady', pre.suggestionReady],
    ['proposalLease', pre.proposalLease],
  ] as const)('%s renders byte-identically and stays read-only', (_name, member) => {
    const actual = existsGuardSql(member.table, member.predicate);
    expect(actual).toBe(member.sql);
    expect(sha(actual)).toBe(member.sha);
    expect(sha(member.sql)).toBe(member.sha);
    readOnly(actual);
  });

  test('the three retained callers share one generator and the suggestion guard is retired', () => {
    expect(shared.match(/export const existsGuardSql\s*=/g) ?? []).toHaveLength(1);
    expect((shared.match(/SELECT /g) ?? []).length).toBe(1);
    expect(records.match(/existsGuardSql\(/g) ?? []).toHaveLength(2);
    expect(requestAi.match(/existsGuardSql\(/g) ?? []).toHaveLength(1);
    expect(records).not.toContain(pre.recordVersion.sql);
    expect(records).not.toContain(pre.companyVersion.sql);
    expect(records).not.toContain(pre.suggestionReady.table);
    expect(requestAi).not.toContain(pre.proposalLease.sql);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('r11-r18 families and all Records source locks stay independent', () => {
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
    const locks = readFileSync(resolve(srcRoot, 'adapters/legal-attachment-read-sources.ts'), 'utf8');
    expect(locks).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(locks).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Records read-tail r19 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r19 BEFORE=33 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r19 deduped the 3x EXISTS single-row guard in records.ts (guard9/guard24)
    // and request-ai.ts into one shared parameterized fragment (33 -> 31);
    // r20 further deduped the SELECT-* by-key reads into one generator (31 -> 30).
    expect(total).toBeLessThanOrEqual(31);
    expect(total).toBeLessThan(33);
  });
});
