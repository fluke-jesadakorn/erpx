import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
// r42/r44 cut the local revision/batch reads onto native kinds; mock the bridge so
// the retained guard oracles stay deterministic without executing SQL.
vi.mock('../../../packages/server-workflow/src/adapters/native-workflow-read', () => ({
  nativeWorkflowReadAll: vi.fn(async () => []),
  nativeWorkflowReadFirst: vi.fn(async () => null),
}));
import { jsonEachApproverPoolSql, approverUnionProjectionSql } from '../../../packages/server-workflow/src/adapters/json-each-sources';
import { participantSql } from '../../../packages/server-workflow/src/adapters/approval-participant-projection';
import { nativeApprovalPlanningPorts } from '../../../packages/server-workflow/src/adapters/approval-planning';
import { nativeFlowPlanningPorts } from '../../../packages/server-workflow/src/adapters/flow-planning-ports';
import { workApprovalSettlementSources } from '../../../packages/server-workflow/src/adapters/work-package-obligation-sources';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
// Guards are built synchronously; no DB execution occurs.
const db = { batch: () => { throw new Error('unexpected batch'); } } as unknown as SqlDatabase;
const approval = nativeApprovalPlanningPorts(db).repository as unknown as {
  guard31: (...args: unknown[]) => { kind: string; values: unknown[] };
};
const flow = nativeFlowPlanningPorts(db).repository as unknown as {
  guard24: (...args: unknown[]) => { kind: string; values: unknown[] };
  guard34: (...args: unknown[]) => { kind: string; values: unknown[] };
};

// Byte oracles captured from HEAD 280a13593ebe9674fb1b46d670d73ac46c42918a before the r48 dedupe.
const stepUnion = "SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))";
const plainUnion = "SELECT approver_user_id UNION SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))";
const participantOracles: string[] = [
  "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='expense' AND ai.record_id=d.id AND ai.company_id=d.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)))",
  "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='task' AND ai.record_id=t.id AND ai.company_id=t.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)))",
  "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='leave' AND ai.record_id=l.id AND ai.company_id=l.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
  "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='leave_cancellation' AND ai.record_id=workforce_native.id AND ai.company_id=workforce_native.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
  "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='overtime' AND ai.record_id=o.id AND ai.company_id=o.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,?,ai.company_id)) AND (ast.state IN ('approved','rejected') OR ast.stage='department_approval' AND jsonb_exists(ast.route_snapshot_json::jsonb,'departmentPolicyVersion') OR ast.stage='hr_approval' AND ast.route_snapshot_json::jsonb->>'appointedPersonId' IS NOT NULL OR EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) aa WHERE aa.id=? AND iam_role_allows(aa.role,aa.company_id,'workflow.'||COALESCE(ast.required_role,CASE WHEN ast.authority IN ('named','department') THEN 'people_manager' ELSE ast.authority END)) AND aa.company_id=ai.company_id AND (aa.scope_kind='holding' OR aa.scope_kind='branch' AND aa.scope_node_id=ai.branch_id OR aa.scope_kind='department' AND aa.scope_node_id=ai.department_id))))",
  "EXISTS(SELECT 1 FROM approval_instances ai JOIN approval_steps ast ON ast.instance_id=ai.id WHERE ai.record_type='leave' AND ai.record_id=l.id AND ai.company_id=l.company_id AND EXISTS(SELECT 1 FROM (SELECT ast.approver_user_id user_id UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant WHERE iam_same_person(participant.user_id,u,ai.company_id)))",
];

describe('approver-union projection dedupe (r48)', () => {
  it('keeps jsonEachApproverPoolSql bytes and closes the union over two variants', () => {
    expect(jsonEachApproverPoolSql('approver_pool_json')).toBe("SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))");
    expect(jsonEachApproverPoolSql('ast.approver_pool_json')).toBe("SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))");
    expect(approverUnionProjectionSql('step')).toBe(stepUnion);
    expect(approverUnionProjectionSql('plain')).toBe(plainUnion);
  });

  it('emits byte-identical participantSql for every caller shape', () => {
    expect(participantSql('expense', 'd')).toBe(participantOracles[0]);
    expect(participantSql('task', 't')).toBe(participantOracles[1]);
    expect(participantSql('leave', 'l', '?', '?')).toBe(participantOracles[2]);
    expect(participantSql('leave_cancellation', 'workforce_native', '?', '?')).toBe(participantOracles[3]);
    expect(participantSql('overtime', 'o', '?', '?')).toBe(participantOracles[4]);
    expect(participantSql('leave', 'l', 'u')).toBe(participantOracles[5]);
  });

  it('keeps guard31 bind order and both emitted SQL variants', () => {
    const override = approval.guard31('key', 'approval-version', 'id', 1, 0, 'id2', 'user', true);
    const plain = approval.guard31('key', 'approval-version', 'id', 1, 0, 'id2', 'user', false);
    expect(override.kind).toBe('wf_native_command_assertions_3');
    expect(plain.kind).toBe('wf_native_command_assertions_3');
    expect(override.values).toEqual(["key","approval-version","id",1,0,"id2","user","EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?) AND EXISTS(SELECT 1 FROM approval_steps WHERE id=? AND state='pending' AND approver_user_id<>?)"]);
    expect(plain.values).toEqual(["key","approval-version","id",1,0,"id2","user","EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?) AND EXISTS(SELECT 1 FROM approval_steps WHERE id=? AND ? IN (SELECT approver_user_id UNION SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))) AND state='pending')"]);
  });
});

describe('flow version-guard dedupe (r48)', () => {
  it('keeps guard24 SQL and bind order', () => {
    const stmt = flow.guard24('key', 'route-version', ['co', 'kind', 'rec'], 9);
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values).toEqual(["key","route-version","co","kind","rec",9,"EXISTS(SELECT 1 FROM flow_routes WHERE company_id=? AND kind=? AND record_id=? AND version=?)"]);
  });

  it('keeps all 11 guard34 document variants and bind order', () => {
    const cases = [
      ["expense", "EXISTS(SELECT 1 FROM finance_expenses WHERE company_id=? AND id=? AND version=?)"],
      ["leave", "EXISTS(SELECT 1 FROM leave_requests WHERE company_id=? AND id=? AND version=?)"],
      ["staff_task", "EXISTS(SELECT 1 FROM staff_tasks WHERE company_id=? AND id=? AND version=?)"],
      ["request", "EXISTS(SELECT 1 FROM employee_requests WHERE company_id=? AND id=? AND version=?)"],
      ["stock_count", "EXISTS(SELECT 1 FROM inventory_counts WHERE company_id=? AND id=? AND version=?)"],
      ["transfer", "EXISTS(SELECT 1 FROM inventory_transfers WHERE company_id=? AND id=? AND version=?)"],
      ["order", "EXISTS(SELECT 1 FROM commerce_orders WHERE company_id=? AND id=? AND version=?)"],
      ["invoice", "EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND id=? AND version=?)"],
      ["journal", "EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND id=? AND version=?)"],
      ["payroll", "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND id=? AND version=?)"],
      ["record", "EXISTS(SELECT 1 FROM workspace_records WHERE company_id=? AND id=? AND version=?)"],
    ] as const;
    for (const [kind, sql] of cases) {
      const stmt = flow.guard34('key', 'flow-document-version', 'co', 'rec', 4, kind);
      expect(stmt.kind).toBe('wf_native_command_assertions_3');
      expect(stmt.values).toEqual(['key', 'flow-document-version', 'co', 'rec', 4, sql]);
    }
    expect(cases).toHaveLength(11);
  });
});

describe('task settlement source dedupe (r48)', () => {
  it('keeps every rendered source byte-identical', () => {
    expect(workApprovalSettlementSources.instances).toBe("SELECT * FROM approval_instances WHERE record_type='task'");
    expect(workApprovalSettlementSources.steps).toBe("SELECT * FROM approval_steps");
    expect(workApprovalSettlementSources.decisions).toBe("SELECT * FROM approval_decisions");
    expect(workApprovalSettlementSources.routineOutbox).toBe("SELECT * FROM outbox_events WHERE event_type='task.generated' AND aggregate_type='staff_task'");
    expect(workApprovalSettlementSources.routineAudits).toBe("SELECT * FROM audit_events WHERE action='routine.occurrence' AND record_type='staff_task'");
  });
});

describe('r48 source retirement pins', () => {
  it('keeps one template literal per deduped family and no writes or locks', async () => {
    const [union, planning, participant, routing, settlement, shared] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/adapters/json-each-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/approval-planning.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/approval-participant-projection.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/flow-planning-ports.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/work-package-obligation-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/closed-projection-sources.ts', import.meta.url), 'utf8'),
    ]);
    expect(union.match(/export const jsonEachApproverPoolSql/g) ?? []).toHaveLength(1);
    expect(union.match(/export const approverUnionProjectionSql/g) ?? []).toHaveLength(1);
    expect(union.match(/SELECT /g) ?? []).toHaveLength(2);
    expect(union).toContain("(variant: 'step' | 'plain')");
    expect(planning).not.toContain('SELECT approver_user_id UNION');
    expect(planning).toContain("approverUnionProjectionSql('plain')");
    expect(participant).not.toContain('SELECT ast.approver_user_id user_id UNION');
    expect(participant).toContain("approverUnionProjectionSql('step')");
    expect(routing.match(/EXISTS\(SELECT 1 FROM \$\{table\}/g) ?? []).toHaveLength(0);
    expect(shared.match(/EXISTS\(SELECT 1 FROM \$\{table\}/g) ?? []).toHaveLength(1);
    expect(routing).toContain("(target: 'route' | FlowKind): string");
    expect(routing).not.toContain('EXISTS(SELECT 1 FROM flow_routes WHERE');
    expect(routing).toContain("flowVersionGuardSql('route')");
    expect(routing).toContain('flowVersionGuardSql(kind)');
    expect(settlement.match(/SELECT /g) ?? []).toHaveLength(1);
    expect(settlement).not.toContain("'SELECT * FROM approval_decisions'");
    expect(union).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const sql of [stepUnion, plainUnion, workApprovalSettlementSources.decisions, workApprovalSettlementSources.routineOutbox, workApprovalSettlementSources.routineAudits]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('retirement:ts-statements is 32 after the r48 cut and r49 receipt dedupe (37->33->32)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(31);
  });
});
