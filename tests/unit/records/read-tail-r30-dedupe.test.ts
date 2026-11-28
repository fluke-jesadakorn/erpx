import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { existsGuardSql } from '../../../modules/records/server/src/adapters/read-tail-exists-sql';
import { ownerVersionExistsSql } from '../../../modules/records/server/src/adapters/read-tail-sql';
import {
  employeeRequestColumnsSql,
  employeeRequestSelectHead,
} from '../../../modules/records/server/src/adapters/read-tail-employee-request-columns';
import { createFlowRoutingSourceRepository } from '../../../modules/records/server/src/adapters/workflow-flow-routing-facts';
import {
  requestAiProposalProjection,
  requestAiProposalProjectionSql,
  requestAiUsageProjection,
  requestAiUsageProjectionSql,
  recordsRequestUsageSql,
} from '../../../modules/records/server/src/adapters/ai-usage-projection';
import { betaManagementRequestSources } from '../../../modules/records/server/src/adapters/beta-management-sources';

const srcRoot = resolve(__dirname, '../../../modules/records/server/src');
const read = (p: string) => readFileSync(resolve(srcRoot, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-r30 rendered SQL SHA-256 oracles, captured from the exact prior source.
// Whole-string byte identity is the oracle: alias, JOIN, table, predicate, tail
// and placeholder/bind order may not drift.
const pre = {
  ownerPlain: '5823bd67f6df6baaeeb970f8146c62d77202525891a74ae5e3c867cbd7e94ee2',
  ownerEmployee: 'c6c6cc7e1b38f078a315c149ebec63c3841b7115104382a2d962e24c9464a74b',
  employeeDetail: 'fb582b0beb8167716a4bf81d7686ee664b72d33e4c62a7c480693d07be912f60',
  employeeLinked: '775ab114fdb19239af2513113af63504f310f658e54f128f92fea600f6bd6fb9',
  flowWorkflow: '6535189ab211a216e48fb5cdf173121ab716294780ba44d33d04fe73fec6dc84',
  flowDefinitions: 'e0e433c3a1633101464928130d656cecdcb958b50ad1b7545825bad054bae325',
  usageFull: '0e3591004b366c0f13a3ca0bf6287d2ca6b398976ffad150bfc8e731f550270d',
  usageMonthly: '6fd832a2d4a78d25870f26bf28312d8176515622eab20e9708b1b3098800c8db',
  usageBeta: '29bf707dc3f062586453362766c44e40bfc4c13b8ab82352d714c39db11224a2',
  proposalFull: '7129a648fc24e19a563a2be2a243b71d846e0ad33cd0c9a534b842c49b02d971',
  proposalDrain: '52fe49c64600a1319632b6a714b15e34dd2cfc42cb87a0dd692417418cfc1505',
} as const;

const proposalDrainTail = " WHERE (state='queued' OR (state='running' AND claim_until<?)) AND attempts<3 AND (?::text IS NULL OR request_id=?) ORDER BY created_at LIMIT 2";

// Capture the real prepared SQL and binds from the flow-routing repository.
const captureFlow = () => {
  const captured: { sql: string; binds: unknown[] }[] = [];
  const db = {
    prepare(sql: string) {
      return {
        bind(...args: unknown[]) {
          captured.push({ sql, binds: args });
          return this;
        },
        all() { return []; },
        first() { return null; },
      };
    },
  };
  const repo = createFlowRoutingSourceRepository(db as never);
  repo.readWorkflowRequests('c1');
  repo.readDefinitionsRequest(1, 'c1');
  return captured;
};

describe('Records read-tail r30 shared-head folds', () => {
  test('#1 ownerVersionExistsSql delegates to existsGuardSql with byte-identical fences', () => {
    const plain = ownerVersionExistsSql('t', '');
    const fence = ownerVersionExistsSql('employee_requests', ' AND state=? AND evidence_version=?');
    expect(plain).toBe(existsGuardSql('t', 'id=? AND company_id=? AND version=?'));
    expect(sha(plain)).toBe(pre.ownerPlain);
    expect(sha(fence)).toBe(pre.ownerEmployee);
    expect(plain).toBe('EXISTS(SELECT 1 FROM t WHERE id=? AND company_id=? AND version=?)');
    expect(fence).toBe('EXISTS(SELECT 1 FROM employee_requests WHERE id=? AND company_id=? AND version=? AND state=? AND evidence_version=?)');
    readOnly(fence);
  });

  test('#2 employee_requests head is shared with the flow-routing JOIN family', () => {
    const detail = employeeRequestColumnsSql('id,title,kind,state', 'parent_request_id=? AND company_id=? ORDER BY created_at LIMIT 50');
    const linked = employeeRequestColumnsSql('id,title', 'company_id=? AND purchase_order_id=?');
    expect(employeeRequestSelectHead('id,title')).toBe('SELECT id,title FROM employee_requests');
    expect(sha(detail)).toBe(pre.employeeDetail);
    expect(sha(linked)).toBe(pre.employeeLinked);
    readOnly(detail);
    readOnly(linked);

    const [workflow, definitions] = captureFlow();
    expect(sha(workflow.sql)).toBe(pre.flowWorkflow);
    expect(sha(definitions.sql)).toBe(pre.flowDefinitions);
    // Binds keep their exact count/order; SQL keeps alias/JOIN/tail bytes.
    expect(workflow.binds).toEqual(['c1']);
    expect(definitions.binds).toEqual([1, 'c1']);
    expect(workflow.sql).toContain('FROM employee_requests r JOIN (');
    expect(workflow.sql).toContain(') f ON f.order_id=r.purchase_order_id JOIN (');
    expect(workflow.sql).toContain(') o ON o.id=f.order_id AND o.company_id=r.company_id WHERE r.company_id=?');
    readOnly(workflow.sql);
    readOnly(definitions.sql);
  });

  test('#3 request_ai_* head is shared with table+tail params', () => {
    const full = requestAiUsageProjection('"id","company_id","actor_id","month","state","lease_until","input_tokens","output_tokens","created_at"');
    const beta = requestAiUsageProjection('company_id,month,input_tokens,output_tokens');
    expect(sha(requestAiUsageProjectionSql)).toBe(pre.usageFull);
    expect(sha(recordsRequestUsageSql)).toBe(pre.usageMonthly);
    expect(sha(full)).toBe(pre.usageFull);
    expect(sha(beta)).toBe(pre.usageBeta);
    expect(sha(betaManagementRequestSources({} as never).aiUsageSource)).toBe(pre.usageBeta);
    expect(sha(requestAiProposalProjectionSql)).toBe(pre.proposalFull);
    expect(sha(requestAiProposalProjection('id', proposalDrainTail))).toBe(pre.proposalDrain);
    expect(sha(full)).toBe(sha(requestAiUsageProjectionSql));
    for (const sql of [requestAiUsageProjectionSql, recordsRequestUsageSql, beta, requestAiProposalProjectionSql, requestAiProposalProjection('id', proposalDrainTail)]) readOnly(sql);
  });

  test('each head is one admitted literal and callers delegate without inlined reads', () => {
    const sql = read('adapters/read-tail-sql.ts');
    const exists = read('adapters/read-tail-exists-sql.ts');
    const emp = read('adapters/read-tail-employee-request-columns.ts');
    const flow = read('adapters/workflow-flow-routing-facts.ts');
    const ai = read('adapters/ai-usage-projection.ts');

    expect(sql.match(/export const ownerVersionExistsSql\s*=/g) ?? []).toHaveLength(1);
    expect(sql).toContain("existsGuardSql(table, `id=? AND company_id=? AND version=?${tail}`)");
    expect((sql.match(/SELECT /g) ?? []).length).toBe(0);
    expect((exists.match(/SELECT /g) ?? []).length).toBe(1);

    expect(emp.split('`SELECT ${columns} FROM employee_requests`')).toHaveLength(1); // r32 delegates to the shared head
    expect((emp.match(/SELECT /g) ?? []).length).toBe(0);
    expect(flow.match(/employeeRequestSelectHead\(/g) ?? []).toHaveLength(1);
    expect(flow.match(/flowRoutingRead\(/g) ?? []).toHaveLength(2);
    expect((flow.match(/SELECT /g) ?? []).length).toBe(0);
    expect(flow).not.toContain(pre.flowWorkflow);
    expect(flow).not.toContain('`SELECT ${columns} FROM employee_requests r JOIN');

    expect(ai.split('`SELECT ${columns} FROM ${table}${tail}`')).toHaveLength(1); // r32 delegates to the shared head
    expect((ai.match(/SELECT /g) ?? []).length).toBe(0); // monthly count folded into r32
    expect(ai).toContain("requestAiProjectionHead('request_ai_usage', columns)");
    expect(ai).toContain("requestAiProjectionHead('request_ai_proposals', columns, tail)");
    expect(read('adapters/request-ai.ts')).not.toContain('SELECT id FROM request_ai_proposals WHERE');

    for (const source of [sql, exists, emp, flow, ai]) {
      expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE/i);
    }
  });

  test('call-site binds and source locks stay untouched', () => {
    expect(read('adapters/employee-requests.ts')).toContain("[id, company_id, version, state, evidence_version]");
    expect(read('adapters/attachments-service-facts.ts')).toContain('[targetId, companyId, version]');
    const locks = read('adapters/legal-attachment-read-sources.ts');
    expect(locks).toContain('LOCK TABLE employee_requests,employee_request_events IN SHARE ROW EXCLUSIVE MODE');
    expect(locks).toContain('LOCK TABLE approval_instances,approval_steps,approval_decisions IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Records read-tail r30 retirement check', () => {
  test('retirement:ts-statements folds three heads (22 -> 19)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // BEFORE=22 (r29). r30 removes one literal from read-tail-sql.ts by
    // delegating to existsGuardSql, one from workflow-flow-routing-facts.ts by
    // sharing the employee_requests head, and one from ai-usage-projection.ts
    // by sharing the request_ai table head. No bind, lock or native seam changed.
    expect(total).toBe(6);
    expect(total).toBeLessThan(22);
  });
});
