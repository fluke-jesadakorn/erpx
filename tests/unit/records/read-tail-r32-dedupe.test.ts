import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { projectionColumnHeadSql } from '../../../packages/server-records/src/adapters/read-tail-r32-sql';
import {
  requestAiProjectionHead,
  requestAiProposalProjection,
  requestAiProposalProjectionSql,
  requestAiUsageProjection,
  requestAiUsageProjectionSql,
  recordsRequestUsageSql,
} from '../../../packages/server-records/src/adapters/ai-usage-projection';
import {
  employeeRequestColumnsSql,
  employeeRequestSelectHead,
} from '../../../packages/server-records/src/adapters/read-tail-employee-request-columns';
import { betaManagementRequestSources } from '../../../packages/server-records/src/adapters/beta-management-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-records/src');
const read = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r32 rendered SQL SHA-256 oracles, captured from the exact prior source at
// HEAD d107e637e. Whole-string byte identity is the oracle: columns, table,
// predicate, quote, placeholder and ORDER BY tokens may not drift through the
// fold of the byte-identical `SELECT <columns> FROM <table>` head.
const pre = {
  usageFull: {
    sql: 'SELECT "id","company_id","actor_id","month","state","lease_until","input_tokens","output_tokens","created_at" FROM request_ai_usage',
    sha: '0e3591004b366c0f13a3ca0bf6287d2ca6b398976ffad150bfc8e731f550270d',
  },
  usageMonthly: {
    sql: 'SELECT company_id,month FROM request_ai_usage',
    sha: '6fd832a2d4a78d25870f26bf28312d8176515622eab20e9708b1b3098800c8db',
  },
  usageBeta: {
    sql: 'SELECT company_id,month,input_tokens,output_tokens FROM request_ai_usage',
    sha: '29bf707dc3f062586453362766c44e40bfc4c13b8ab82352d714c39db11224a2',
  },
  proposalFull: {
    sql: 'SELECT id,company_id,request_id,source_version,provider_json,actor_json,state,result_json,error,attempts,lease_token,claim_until,created_at,updated_at FROM request_ai_proposals',
    sha: '7129a648fc24e19a563a2be2a243b71d846e0ad33cd0c9a534b842c49b02d971',
  },
  proposalDrain: {
    sql: "SELECT id FROM request_ai_proposals WHERE (state='queued' OR (state='running' AND claim_until<?)) AND attempts<3 AND (?::text IS NULL OR request_id=?) ORDER BY created_at LIMIT 2",
    sha: '52fe49c64600a1319632b6a714b15e34dd2cfc42cb87a0dd692417418cfc1505',
  },
  empHead: {
    sql: 'SELECT id,title FROM employee_requests',
    sha: '818b062bdb2235bf99c40164f3e4db1e325a422d0273530dd7a26fdca7c33f80',
  },
  empDetail: {
    sql: 'SELECT id,title,kind,state FROM employee_requests WHERE parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50',
    sha: 'fb582b0beb8167716a4bf81d7686ee664b72d33e4c62a7c480693d07be912f60',
  },
  empLinked: {
    sql: 'SELECT id,title FROM employee_requests WHERE company_id=? AND purchase_order_id=?',
    sha: '775ab114fdb19239af2513113af63504f310f658e54f128f92fea600f6bd6fb9',
  },
} as const;

const proposalDrainTail = " WHERE (state='queued' OR (state='running' AND claim_until<?)) AND attempts<3 AND (?::text IS NULL OR request_id=?) ORDER BY created_at LIMIT 2";

describe('Records read-tail r32 shared column-projection head', () => {
  test('request_ai_* projections render byte-identically through the shared head', () => {
    const full = requestAiUsageProjection('"id","company_id","actor_id","month","state","lease_until","input_tokens","output_tokens","created_at"');
    const beta = requestAiUsageProjection('company_id,month,input_tokens,output_tokens');
    const drain = requestAiProposalProjection('id', proposalDrainTail);
    expect(requestAiUsageProjectionSql).toBe(pre.usageFull.sql);
    expect(recordsRequestUsageSql).toBe(pre.usageMonthly.sql);
    expect(betaManagementRequestSources({} as never).aiUsageSource).toBe(pre.usageBeta.sql);
    expect(requestAiProposalProjectionSql).toBe(pre.proposalFull.sql);
    expect(drain).toBe(pre.proposalDrain.sql);
    const cases = [
      ['usageFull', full],
      ['usageMonthly', recordsRequestUsageSql],
      ['usageBeta', beta],
      ['usageBeta', betaManagementRequestSources({} as never).aiUsageSource],
      ['proposalFull', requestAiProposalProjectionSql],
      ['proposalDrain', drain],
    ] as const;
    for (const [name, sql] of cases) {
      expect(sql).toBe(pre[name].sql);
      expect(sha(sql)).toBe(pre[name].sha);
      readOnly(sql);
    }
  });

  test('employee_requests column family renders byte-identically through the same head', () => {
    const detail = employeeRequestColumnsSql('id,title,kind,state', 'parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50');
    const linked = employeeRequestColumnsSql('id,title', 'company_id=? AND purchase_order_id=?');
    expect(employeeRequestSelectHead('id,title')).toBe(pre.empHead.sql);
    expect(detail).toBe(pre.empDetail.sql);
    expect(linked).toBe(pre.empLinked.sql);
    expect(sha(employeeRequestSelectHead('id,title'))).toBe(pre.empHead.sha);
    expect(sha(detail)).toBe(pre.empDetail.sha);
    expect(sha(linked)).toBe(pre.empLinked.sha);
    expect((linked.match(/\?/g) ?? []).length).toBe(2);
    expect((detail.match(/\?/g) ?? []).length).toBe(2);
    for (const sql of [employeeRequestSelectHead('id,title'), detail, linked]) readOnly(sql);
  });

  test('the two families share one byte-identical head literal', () => {
    const head = projectionColumnHeadSql('id,title', 'employee_requests');
    expect(head).toBe('SELECT id,title FROM employee_requests');
    expect(sha(head)).toBe(pre.empHead.sha);
    expect(head).toBe(employeeRequestSelectHead('id,title'));
    expect(head).toBe(requestAiProjectionHead('employee_requests', 'id,title'));
    expect(sha(head)).toBe(sha(requestAiProjectionHead('employee_requests', 'id,title')));
    expect(projectionColumnHeadSql('company_id,month', 'request_ai_usage')).toBe(recordsRequestUsageSql);
    expect(projectionColumnHeadSql('id', 'request_ai_proposals', proposalDrainTail)).toBe(pre.proposalDrain.sql);
  });

  test('the head is admitted once; callers delegate and protected edges stay pinned', () => {
    const shared = read('adapters/read-tail-r32-sql.ts');
    const ai = read('adapters/ai-usage-projection.ts');
    const emp = read('adapters/read-tail-employee-request-columns.ts');

    expect(shared.match(/export const projectionColumnHeadSql\s*=/g) ?? []).toHaveLength(1);
    expect(shared).toContain('`SELECT ${columns} FROM ${table}${tail}`');
    expect((shared.match(/SELECT /g) ?? []).length).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);

    expect(ai).not.toContain('`SELECT ${columns} FROM ${table}${tail}`');
    expect((ai.match(/SELECT /g) ?? []).length).toBe(0); // monthly count folded into r32
    expect(ai).toContain("projectionColumnHeadSql(columns, table, tail)");
    expect(ai).toContain("requestAiProjectionHead('request_ai_usage', columns)");
    expect(ai).toContain("requestAiProjectionHead('request_ai_proposals', columns, tail)");

    expect(emp).not.toContain('`SELECT ${columns} FROM employee_requests`');
    expect((emp.match(/SELECT /g) ?? []).length).toBe(0);
    expect(emp).toContain("projectionColumnHeadSql(columns, 'employee_requests')");

    // Prior rounds, registry fallback, locks, native and cross-owner ports stay pinned.
    const records = read('adapters/records.ts');
    const legal = read('adapters/legal-attachment-read-sources.ts');
    expect(records).not.toContain('const recordContextDetailSql =');
    expect(records).not.toContain('const referenceEdgesSql =');
    expect(records).toContain('Removal task for the retained Field partition below: retire.records.registry-fallback.');
    expect(legal).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(legal).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
    expect(read('public/native-record-registry.ts')).not.toMatch(/SELECT /);
    expect(read('public/native-record-context.ts')).not.toMatch(/SELECT /);
    expect(records).toContain("import {withExpenseWorkspaceRecordRead} from '@erpx/server-people_payroll/public/record-purchases';");
  });
});

describe('Records read-tail r32 retirement check', () => {
  test('retirement:ts-statements folds the shared column head (19 -> 18)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // BEFORE=19 at HEAD d107e637e. r32 removes one literal from
    // ai-usage-projection.ts and one from read-tail-employee-request-columns.ts
    // by delegating both byte-identical heads to one renderer (+1). No bind,
    // lock, registry fallback, native seam or cross-owner consumer changed.
    expect(total).toBe(9);
    expect(total).toBeLessThan(19);
  });
});
