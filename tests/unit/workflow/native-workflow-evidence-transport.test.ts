import { readFile } from 'node:fs/promises';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeWorkflowEvidenceClient,
  nativeWorkflowEvidenceConfiguration,
  nativeWorkflowEvidenceFailure,
  NativeWorkflowEvidenceError,
  type NativeWorkflowEvidenceActor,
  type NativeWorkflowEvidenceConfiguration,
} from '../../../modules/workflow/server/src/public/native-workflow-evidence';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44993',
    timeoutMs: 1000,
    holdings: [
      { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
    ],
    ...overrides,
  });
const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.com',
    cookie: '__Host-erpx-product=session',
    ...overrides,
  });
const actor: NativeWorkflowEvidenceActor = {
  userId: 'actor',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-actor',
  role: 'organization_admin',
  scopeKind: 'holding',
  authority: { revision: 9, assignmentVersion: 3 },
  permissions: ['organization.manage'],
};
const context = {
  userId: 'actor',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-actor',
  assignmentVersion: 3,
  revision: 9,
  role: 'organization_admin',
  scopeKind: 'holding',
  permissions: ['organization.manage'],
};
const queueRow = {
  id: 'approval-1',
  company_id: 'company',
  branch_id: null,
  department_id: null,
  record_type: 'expense',
  record_id: 'expense-1',
  state: 'pending',
  blocked_reason: null,
  version: 2,
  created_at: 100,
};
const instance = {
  id: 'approval-1',
  company_id: 'company',
  branch_id: null,
  department_id: null,
  record_type: 'expense',
  record_id: 'expense-1',
  phase: 'request',
  cycle: 1,
  requester_user_id: 'requester',
  assignee_user_id: null,
  state: 'pending',
  current_step: 0,
  blocked_reason: null,
  version: 2,
  created_at: 100,
  updated_at: 200,
};
const step = {
  id: 'step-0',
  instance_id: 'approval-1',
  ordinal: 0,
  approver_user_id: 'approver',
  authority: 'named',
  state: 'pending',
  approver_pool_json: null,
  required_role: null,
  stage: null,
  route_snapshot_json: null,
};
const decision = {
  id: 'decision-1',
  instance_id: 'approval-1',
  step_id: null,
  action: 'initialize',
  actor_user_id: 'system',
  previous_approver_user_id: null,
  approver_user_id: null,
  reason: 'Initialize route',
  reviewed_version: 1,
  command_id: 'approval:initialize:1',
  created_at: 110,
};
const receipt = {
  command_id: 'approval:initialize:1',
  operation: 'approval.initialize',
  actor_id: 'system',
  record_id: 'expense-1',
  request_hash: 'a'.repeat(64),
  result_json: '{"state":"pending"}',
  created_at: 110,
};
const audit = {
  id: 'audit-1',
  command_id: 'approval:initialize:1',
  actor_id: 'system',
  assignment_id: 'assignment-system',
  action: 'approval.initialize',
  record_type: 'expense_approval',
  record_id: 'expense-1',
  record_version: 1,
  purpose: 'approval.initialize',
  created_at: 110,
};
const outbox = {
  id: 'outbox-1',
  command_id: 'approval:initialize:1',
  event_type: 'approval.initialize',
  aggregate_type: 'expense_approval',
  aggregate_id: 'expense-1',
  aggregate_version: 1,
  created_at: 110,
};
const approval = {
  instance,
  steps: [step],
  history: [decision],
  receipts: [receipt],
  audit: [audit],
  outbox: [outbox],
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const client = (raw = config(), transport = vi.fn()) =>
  createNativeWorkflowEvidenceClient(
    nativeWorkflowEvidenceConfiguration(raw) as NativeWorkflowEvidenceConfiguration,
    transport,
  );

describe('native Workflow approval evidence transport', () => {
  it('forwards the registered host, cookie and context, and returns the native queue', async () => {
    const transport = vi.fn().mockResolvedValue(
      ok({ data: { evaluatedAt: 500, approvals: [queueRow] }, context }),
    );
    const result = await client(config(), transport).queue(incoming(), actor);
    expect(result.approvals[0]?.record_id).toBe('expense-1');
    expect(transport).toHaveBeenCalledTimes(1);
    const [method, url, headers, body] = transport.mock.calls[0]!;
    expect(method).toBe('GET');
    expect(String(url)).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/workflow/approvals');
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-assignment')).toBe('assignment-actor');
    expect(headers.get('x-erpx-context-revision')).toBe('9');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(body).toBeUndefined();
  });

  it('returns the exact approval evidence and path for one record', async () => {
    const transport = vi.fn().mockResolvedValue(
      ok({ data: { evaluatedAt: 500, approval }, context }),
    );
    const result = await client(config(), transport).evidence(
      incoming(),
      actor,
      'expense',
      'expense-1',
    );
    expect(result.approval?.instance.record_id).toBe('expense-1');
    expect(result.approval?.history[0]?.command_id).toBe('approval:initialize:1');
    const [, url] = transport.mock.calls[0]!;
    expect(String(url)).toBe(
      'http://127.0.0.1:44993/api/v1/holdings/acme/workflow/approvals/expense/expense-1/evidence',
    );
  });

  it('rejects a changed context, foreign company rows and malformed evidence', async () => {
    const changed = vi.fn().mockResolvedValue(
      ok({ data: { evaluatedAt: 500, approvals: [] }, context: { ...context, revision: 10 } }),
    );
    await expect(client(config(), changed).queue(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_WORKFLOW_EVIDENCE_CONTEXT_CHANGED',
    });

    const foreign = vi.fn().mockResolvedValue(
      ok({ data: { evaluatedAt: 500, approvals: [{ ...queueRow, company_id: 'other' }] }, context }),
    );
    await expect(client(config(), foreign).queue(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID',
    });

    const extra = vi.fn().mockResolvedValue(
      ok({ data: { evaluatedAt: 500, approval: { ...approval, extra: true } }, context }),
    );
    await expect(
      client(config(), extra).evidence(incoming(), actor, 'expense', 'expense-1'),
    ).rejects.toMatchObject({ code: 'NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID' });

    const orphan = vi.fn().mockResolvedValue(
      ok({
        data: { evaluatedAt: 500, approval: { ...approval, receipts: [{ ...receipt, command_id: 'unlinked' }] } },
        context,
      }),
    );
    await expect(
      client(config(), orphan).evidence(incoming(), actor, 'expense', 'expense-1'),
    ).rejects.toMatchObject({ code: 'NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID' });

    const duplicate = vi.fn().mockResolvedValue(
      ok({ data: { evaluatedAt: 500, approvals: [queueRow, queueRow] }, context }),
    );
    await expect(client(config(), duplicate).queue(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID',
    });
  });

  it('rejects foreign hosts, reserved configurations and non-JSON or failed responses', async () => {
    const transport = vi.fn();
    await expect(
      client(config(), transport).queue(incoming({ host: 'other.example.com' }), actor),
    ).rejects.toMatchObject({ code: 'NATIVE_WORKFLOW_EVIDENCE_AUTHORITY_INVALID' });
    expect(transport).not.toHaveBeenCalled();

    for (const raw of [
      config({ privateBaseUrl: 'http://127.0.0.1:3100' }),
      config({ privateBaseUrl: 'http://localhost:44993' }),
      config({ privateBaseUrl: 'https://127.0.0.1:44993' }),
      config({ holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }] }),
      config({ holdings: [
        { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
        { origin: 'https://acme.example.com', holdingSlug: 'other', holdingId: 'h002' },
      ] }),
      config({ timeoutMs: 249 }),
    ]) {
      expect(() => nativeWorkflowEvidenceConfiguration(raw)).toThrow();
    }

    const failed = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ error: { code: 'APPROVAL_EVIDENCE_PERMISSION_REQUIRED', message: 'Denied.' } }), {
        status: 403,
        headers: { 'content-type': 'application/json' },
      }),
    );
    await expect(client(config(), failed).queue(incoming(), actor)).rejects.toMatchObject({
      code: 'APPROVAL_EVIDENCE_PERMISSION_REQUIRED',
      status: 403,
    });

    const text = vi.fn().mockResolvedValue(
      new Response('not json', { status: 200, headers: { 'content-type': 'text/plain' } }),
    );
    await expect(client(config(), text).queue(incoming(), actor)).rejects.toMatchObject({
      code: 'NATIVE_WORKFLOW_EVIDENCE_RESPONSE_INVALID',
    });
  });

  it('maps owner failures without a TypeScript fallback', () => {
    expect(nativeWorkflowEvidenceFailure({ code: 'x' })).toBeUndefined();
    expect(
      nativeWorkflowEvidenceFailure(
        new NativeWorkflowEvidenceError('NATIVE_WORKFLOW_EVIDENCE_CONTEXT_CHANGED'),
      ),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(
      nativeWorkflowEvidenceFailure(
        new NativeWorkflowEvidenceError('NATIVE_WORKFLOW_EVIDENCE_CONFIGURATION_INVALID'),
      ),
    ).toMatchObject({ status: 503, code: 'NATIVE_WORKFLOW_EVIDENCE_UNAVAILABLE' });
    expect(
      nativeWorkflowEvidenceFailure(
        new NativeWorkflowEvidenceError(
          'APPROVAL_EVIDENCE_PERMISSION_REQUIRED',
          403,
          'Denied.',
        ),
      ),
    ).toMatchObject({ status: 403, code: 'APPROVAL_EVIDENCE_PERMISSION_REQUIRED' });
  });
});

describe('workflow Work UI staff read tail (native read plan)', () => {
  it('plans both reads through the mounted native read with no local SQL', async () => {
    const rows = [{ id: 'a' }];
    const beta = { '1': 1 };
    const all = vi.fn(async () => rows);
    const first = vi.fn(async () => beta);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowReadFirst: first,
    }));
    try {
      const { workUiStaffFacts } = await import('../../../modules/workflow/server/src/adapters/work-ui-staff-facts');
      const prepare = vi.fn();
      const db = { prepare } as never;
      const facts = workUiStaffFacts(db);
      expect(await facts.approvalQueue('company', 0)).toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith('wf_native_work_ui_approval_queue', ['company', 0], 'company', db);
      expect(await facts.betaApproval('approval', 'company')).toEqual(beta);
      expect(first).toHaveBeenCalledWith('wf_native_work_ui_beta_approval', ['approval', 'company'], 'company', db);
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow spending settings history read tail (native read plan)', () => {
  it('carries no retired owner SQL in the switched adapter', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/flow-settings-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT');
    expect(source).not.toContain('audit_events');
    expect(source).not.toContain('.prepare(');
  });

  it('plans the read through the mounted native read with no local SQL', async () => {
    const rows = [{ title: 'Company spending rules', version: 2, actor: 'actor', at: 100, kind: 'Spending rules revision' }];
    const all = vi.fn(async () => rows);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
    }));
    try {
      const { spendingSettingsHistory } = await import(
        '../../../modules/workflow/server/src/adapters/flow-settings-facts'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await spendingSettingsHistory(db, 'company')).toEqual(rows);
      expect(all).toHaveBeenCalledWith(
        'wf_native_spending_settings_history',
        ['company', 'company'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow staff task alert approvers read tail (native read plan)', () => {
  it('carries no retired owner SQL in the switched adapter', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/operations-staff-task-alerts-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT');
    expect(source).not.toContain('approval_instances');
    expect(source).not.toContain('.prepare(');
  });

  it('plans the read through the mounted native read with no local SQL', async () => {
    const rows = [{ approver_user_id: 'approver' }];
    const all = vi.fn(async () => rows);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
    }));
    try {
      const { StaffTaskAlertsWorkflowFacts } = await import(
        '../../../modules/workflow/server/src/adapters/operations-staff-task-alerts-facts'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await StaffTaskAlertsWorkflowFacts(db).readEscalateStaffTasksApprovers('company', 'task'))
        .toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith(
        'wf_native_staff_task_alert_approvers',
        ['company', 'task'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow department approval queue read tail (native read plan)', () => {
  it('carries no retired owner SQL for the switched read', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/department-override-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT i.record_type kind,i.record_id id,i.updated_at');
    expect(source).not.toContain('ORDER BY i.updated_at DESC LIMIT 150');
  });

  it('plans the read through the mounted native read with no local SQL', async () => {
    const rows = [{ kind: 'expense', id: 'request-1', updated_at: 100 }];
    const all = vi.fn(async () => rows);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
    }));
    try {
      const { departmentOverridePendingApprovals } = await import(
        '../../../modules/workflow/server/src/adapters/department-override-facts'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await departmentOverridePendingApprovals(db, 'company')).toEqual(rows);
      expect(all).toHaveBeenCalledWith(
        'wf_native_department_approval_queue',
        ['company'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow work report facts read tail (native read plan)', () => {
  it('carries no retired owner SQL for the switched reads', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/work-report-facts.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('JOIN approval_decisions d ON d.instance_id=i.id');
    expect(source).not.toContain('jsonb_array_elements_text(?::jsonb)');
  });

  it('plans both reads through the mounted native read with no local SQL', async () => {
    const decisions = [{ record_id: 'record-1', id: 'd1' }];
    const routes = [{ record_id: 'record-1', cycle: 1, ordinal: 0 }];
    const all = vi.fn(async (kind: string) =>
      kind === 'wf_native_work_report_decisions' ? decisions : routes,
    );
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
    }));
    try {
      const { workReportWorkflowFacts } = await import(
        '../../../modules/workflow/server/src/adapters/work-report-facts'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const facts = workReportWorkflowFacts(db);
      expect(await facts.decisions('company', '["record-1"]')).toEqual({ results: decisions });
      expect(await facts.savedRoutes('company', '["record-1"]')).toEqual({ results: routes });
      expect(all).toHaveBeenNthCalledWith(
        1,
        'wf_native_work_report_decisions',
        ['company', '["record-1"]'],
        'company',
        db,
      );
      expect(all).toHaveBeenNthCalledWith(
        2,
        'wf_native_work_report_saved_routes',
        ['company', '["record-1"]'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow approval administration queue read tail (native read plan)', () => {
  it('carries no retired owner SQL for the switched read', async () => {
    const source = await readFile(
      new URL(
        '../../../modules/workflow/server/src/adapters/approval-administration.ts',
        import.meta.url,
      ),
      'utf8',
    );
    expect(source).not.toContain('SELECT record_type,record_id FROM approval_instances');
    expect(source).not.toContain('?::text IS NULL OR (record_type=? AND record_id=?)');
  });

  it('plans the queue read through the mounted native read with no local SQL', async () => {
    const all = vi.fn(async () => [] as { record_type: string; record_id: string }[]);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
    }));
    // Force the non-evidence fallback branch that owns this read, without
    // requiring the native evidence client configuration or other bundle imports.
    vi.doMock('@erpx/workflow-server/public/native-workflow-evidence', () => ({
      nativeWorkflowEvidenceAvailable: () => false,
      nativeWorkflowEvidenceConfiguration: () => ({}),
      nativeWorkflowEvidenceFailure: () => null,
      createNativeWorkflowEvidenceClient: () => ({}),
    }));
    vi.doMock('@erpx/operations-server/public/staff-settings', () => ({
      staffSettings: async () => ({ levels: [], people: [], departments: [], canManage: true }),
    }));
    vi.doMock('next/headers', () => ({ headers: async () => new Headers() }));
    try {
      const { approvalAdministrationData } = await import(
        '../../../modules/workflow/server/src/adapters/approval-administration'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const actor = {
        userId: 'actor',
        holdingId: 'h001',
        holdingSlug: 'acme',
        companyId: 'company',
        branchId: null,
        departmentId: null,
        assignmentId: 'assignment-actor',
        role: 'organization_admin',
        scopeKind: 'holding',
        authority: { revision: 9, assignmentVersion: 3 },
        iam: { permissions: [] },
      } as never;
      const result = await approvalAdministrationData(actor, undefined, undefined, db);
      expect(result?.rows).toEqual([]);
      expect(all).toHaveBeenCalledWith(
        'wf_native_approval_administration_queue',
        ['company', null, null, null],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.doUnmock('@erpx/workflow-server/public/native-workflow-evidence');
      vi.doUnmock('@erpx/operations-server/public/staff-settings');
      vi.doUnmock('next/headers');
      vi.resetModules();
    }
  });
});

describe('workflow legal decision history read tail (native read plan)', () => {
  it('carries no retired owner SQL for the switched read', async () => {
    const source = await readFile(
      new URL(
        '../../../modules/workflow/server/src/adapters/legal-approval-facts.ts',
        import.meta.url,
      ),
      'utf8',
    );
    expect(source).not.toContain('SELECT d.* FROM approval_decisions d JOIN approval_instances i');
    expect(source).not.toContain("i.record_type='legal' AND i.record_id=? AND i.company_id=?");
    expect(source).not.toContain('ORDER BY d.insertion_order');
  });

  it('plans the decision history read through the mounted native read with no local SQL', async () => {
    const rows = [{ id: 'decision-1', action: 'approve', actor_user_id: 'actor' }];
    const all = vi.fn(async () => rows);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
    }));
    try {
      const { legalApprovalFacts } = await import(
        '../../../modules/workflow/server/src/adapters/legal-approval-facts'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await legalApprovalFacts(db).decisionHistory({ id: 'legal-1', companyId: 'company' }))
        .toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith(
        'wf_native_legal_decision_history',
        ['legal-1', 'company'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow flow planning definitions instance read tail (native read plan)', () => {
  it('carries no retired owner SQL for the switched read', async () => {
    const source = await readFile(
      new URL(
        '../../../modules/workflow/server/src/adapters/flow-planning-ports.ts',
        import.meta.url,
      ),
      'utf8',
    );
    expect(source).not.toContain('SELECT * FROM approval_instances WHERE company_id=? AND record_type=? AND record_id=?');
    expect(source).not.toContain('ORDER BY insertion_order DESC LIMIT 1").bind(company_id, approvalKind, id)');
  });

  it('plans the definitions instance read through the mounted native read with no local SQL', async () => {
    const row = { id: 'instance-1', record_type: 'expense' };
    const first = vi.fn(async () => row);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadFirst: first,
    }));
    try {
      const { nativeFlowPlanningPorts } = await import(
        '../../../modules/workflow/server/src/adapters/flow-planning-ports'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const ports = nativeFlowPlanningPorts(db);
      expect(await ports.repository.readDefinitionsInstance('company', 'expense', 'record-1'))
        .toEqual(row);
      expect(first).toHaveBeenCalledWith(
        'wf_native_flow_definitions_instance',
        ['company', 'expense', 'record-1'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow finance sales completion receipt read tail (native read plan)', () => {
  it('carries no retired owner SQL for the switched read', async () => {
    const source = await readFile(
      new URL(
        '../../../modules/workflow/server/src/adapters/finance-command-receipts.ts',
        import.meta.url,
      ),
      'utf8',
    );
    expect(source).not.toContain("SELECT result_json FROM command_receipts WHERE record_id=? AND operation='finance.sales-invoice.prepare'");
    expect(source).not.toContain("authority_json::jsonb->>'companyId'=? ORDER BY created_at DESC LIMIT 1");
  });

  it('plans the sales completion receipt read through the mounted native read with no local SQL', async () => {
    const row = { result_json: '{"status":"posted"}' };
    const first = vi.fn(async () => row);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadFirst: first,
    }));
    try {
      const { financeCommandReceipts } = await import(
        '../../../modules/workflow/server/src/adapters/finance-command-receipts'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await financeCommandReceipts(db).salesCompletion('company', 'invoice-1')).toEqual(row);
      expect(first).toHaveBeenCalledWith(
        'wf_native_finance_sales_completion',
        ['invoice-1', 'company'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow finance AI source revision read tail (native read plan)', () => {
  it('carries no retired owner read SQL in the switched adapter', async () => {
    const source = await readFile(
      new URL(
        '../../../modules/workflow/server/src/adapters/finance-ai-revision-sources.ts',
        import.meta.url,
      ),
      'utf8',
    );
    expect(source).not.toContain('AS revision');
    expect(source).not.toContain('.prepare(');
  });

  it('plans the source revision read through the mounted native read with no local SQL', async () => {
    const first = vi.fn(async () => ({ revision: 7 }));
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadFirst: first,
    }));
    try {
      const { financeSourceRevision } = await import(
        '../../../modules/workflow/server/src/adapters/finance-ai-revision-sources'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await financeSourceRevision(db, 'company')).toBe(7);
      expect(first).toHaveBeenCalledWith(
        'wf_native_finance_source_revision',
        ['company'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow project worker command evidence image read tail (native read plan)', () => {
  it('retains the cross-owner assertion fragment but carries no retired owner read SQL', async () => {
    const source = await readFile(
      new URL(
        '../../../modules/workflow/server/src/adapters/project-worker-startup-evidence.ts',
        import.meta.url,
      ),
      'utf8',
    );
    expect(source).toContain("commandReceiptsProjectionSql(['command_id','operation','record_id','actor_id','authority_json','result_json'])");
    expect(source).not.toContain("'SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM command_receipts'");
    expect(source).not.toContain('jsonb_build_object');
    expect(source).not.toContain('.prepare(');
  });

  it('plans the evidence image read through the mounted native read with no local SQL', async () => {
    const first = vi.fn(async () => ({ image: 'image-1' }));
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadFirst: first,
    }));
    try {
      const { projectWorkerCommandEvidenceImage } = await import(
        '../../../modules/workflow/server/src/adapters/project-worker-startup-evidence'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      expect(await projectWorkerCommandEvidenceImage(db, 'company', 'record-1')).toBe('image-1');
      expect(first).toHaveBeenCalledWith(
        'wf_native_project_worker_command_evidence',
        ['record-1', 'company', 'record-1', 'company', 'record-1', 'company'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow canonical replay receipt read tail (native read plan)', () => {
  it('carries no retired owner SQL for the switched read', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/commands.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT request_hash,result_json,operation,actor_id,record_id,authority_json FROM command_receipts');
    expect(source).not.toContain('.prepare(');
    expect(source).toContain("'wf_native_command_receipt_replay'");
  });

  it('plans the replay receipt read through the mounted native read with no local SQL', async () => {
    const row = {
      request_hash: 'hash', result_json: '{"ok":true}', operation: 'op',
      actor_id: 'actor', record_id: 'record', authority_json: null,
    };
    const first = vi.fn(async () => row);
    let repository: { receipt: (id: string) => Promise<unknown> } | undefined;
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadFirst: first,
    }));
    vi.doMock('@erpx/workflow-server/application/commands', () => ({
      createCommandApplication: (ports: { repository: typeof repository }) => {
        repository = ports.repository;
        return { replayCommand: async () => null, executeCommand: async () => null };
      },
    }));
    vi.doMock('@erpx/collaboration-server/adapters/native-command-assertions', () => ({
      nativeClearCommandAssertions: async () => ({}),
    }));
    vi.doMock('@erpx/platform-runtime-server/public/http', () => ({
      ApiError: class ApiError extends Error {},
    }));
    vi.doMock('@erpx/identity-access-server/public/iam-store', () => ({
      iamMutationGuards: () => [],
    }));
    vi.doMock('@erpx/identity-access-server/public/workflow-command-authority', () => ({
      createWorkflowCommandGuards: () => ({
        workflowWorkContextGuards: () => [], workflowAssignmentGuard: () => ({}),
      }),
      workflowActiveContext: async () => null,
      workflowActiveGrant: async () => true,
    }));
    vi.doMock('@erpx/workflow-server/public/command-assertions', () => ({ assertion: {} }));
    vi.doMock('@erpx/workflow-server/public/command-policies', () => ({
      prepareOwnerCommandStatements: async () => [],
    }));
    vi.doMock('@erpx/platform-shared-server/index', () => ({
      BusinessRuleError: class BusinessRuleError extends Error {},
    }));
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-wave3', () => ({
      workflowWave3Statement: () => ({}),
    }));
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-write', () => ({
      resolveNativeEffects: async () => ({ statements: [] }),
    }));
    try {
      const { replayCommand } = await import('../../../modules/workflow/server/src/adapters/commands');
      const prepare = vi.fn();
      const db = { prepare } as never;
      await replayCommand(db, { companyId: 'company' } as never, 'command-1', 'op', 'record', {});
      expect(repository).toBeDefined();
      expect(await repository!.receipt('command-1')).toEqual(row);
      expect(first).toHaveBeenCalledWith(
        'wf_native_command_receipt_replay',
        ['command-1'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      for (const specifier of [
        '../../../modules/workflow/server/src/adapters/native-workflow-read',
        '@erpx/workflow-server/application/commands',
        '@erpx/collaboration-server/adapters/native-command-assertions',
        '@erpx/platform-runtime-server/public/http',
        '@erpx/identity-access-server/public/iam-store',
        '@erpx/identity-access-server/public/workflow-command-authority',
        '@erpx/workflow-server/public/command-assertions',
        '@erpx/workflow-server/public/command-policies',
        '@erpx/platform-shared-server/index',
        '../../../modules/workflow/server/src/adapters/native-workflow-wave3',
        '../../../modules/workflow/server/src/adapters/native-workflow-write',
      ]) {
        vi.doUnmock(specifier as string);
      }
      vi.resetModules();
    }
  });
});

describe('workflow export receipt result_json dedupe (r32)', () => {
  it('keeps one fixed SQL constant for the remaining export receipt reader', async () => {
    const [financeSource, sharedSource] = await Promise.all([
      readFile(
        new URL(
          '../../../modules/workflow/server/src/adapters/finance-command-receipts.ts',
          import.meta.url,
        ),
        'utf8',
      ),
      readFile(
        new URL('../../../modules/workflow/server/src/adapters/receipt-sql.ts', import.meta.url),
        'utf8',
      ),
    ]);
    expect(financeSource).toContain('exportReceiptResultSql');
    expect(financeSource).not.toContain("AND operation='finance.refund.export'");
    expect(sharedSource).toContain("commandReceiptsProjectionSql(['result_json'], ',', ' WHERE command_id=? AND operation=?')");
    expect(sharedSource.match(/SELECT/g) ?? []).toHaveLength(0);
  });

  it('leaves no orphan adapter/barrel or dangling export entry', async () => {
    const { access } = await import('node:fs/promises');
    for (const relative of [
      '../../../modules/workflow/server/src/adapters/supplier-export-receipt.ts',
      '../../../modules/workflow/server/src/public/supplier-export-receipt.ts',
    ]) {
      await expect(access(new URL(relative, import.meta.url))).rejects.toThrow();
    }
    const manifest = await readFile(new URL('../../../modules/workflow/server/package.json', import.meta.url), 'utf8');
    expect(manifest).not.toContain('supplier-export-receipt');
  });

  it('binds command id and operation and returns result_json for the refund reader', async () => {
    const { exportReceiptResultSql } = await import(
      '../../../modules/workflow/server/src/adapters/receipt-sql'
    );
    const { financeCommandReceipts } = await import(
      '../../../modules/workflow/server/src/adapters/finance-command-receipts'
    );
    const row = { result_json: '{"exported":true}' };
    const fakeDb = () => {
      const first = vi.fn(async () => row);
      const bind = vi.fn(() => ({ first }));
      const prepare = vi.fn(() => ({ bind }));
      return { db: { prepare } as never, prepare, bind, first };
    };

    const refund = fakeDb();
    expect(await financeCommandReceipts(refund.db).refundExport('refund-key')).toEqual(row);
    expect(refund.prepare).toHaveBeenCalledWith(exportReceiptResultSql);
    expect(refund.bind).toHaveBeenCalledWith('refund-key', 'finance.refund.export');
  });
});

describe('workflow flow-routes read tail (r41 native read plan)', () => {
  it('carries no retired owner SQL for the switched reads', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/flow-planning-ports.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('flowRoutesByCompanySql');
    expect(source).not.toContain('SELECT * FROM flow_routes WHERE company_id=?');
    expect(source).not.toContain('AND kind=? AND record_id=?`).bind(companyId, kind, id)');
  });

  it('plans both flow-routes reads through the mounted native read with no local SQL', async () => {
    const rows = [{ kind: 'expense', record_id: 'record-1' }];
    const all = vi.fn(async () => rows);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowReadFirst: vi.fn(),
    }));
    try {
      const { nativeFlowPlanningPorts } = await import(
        '../../../modules/workflow/server/src/adapters/flow-planning-ports'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const repo = nativeFlowPlanningPorts(db).repository;
      expect(await repo.readSharedFlowReferencesRoutesdefaultsevents('company')).toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith(
        'wf_native_flow_routes_by_company',
        ['company'],
        'company',
        db,
      );
      expect(await repo.readReadFlowRoute('company', 'expense', 'record-1')).toEqual(rows[0]);
      expect(all).toHaveBeenCalledWith(
        'wf_native_flow_routes_by_company_kind_record',
        ['company', 'expense', 'record-1'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });

  it('keeps the by-id read first-row parity when the native plan returns no rows', async () => {
    const all = vi.fn(async () => [] as unknown[]);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowReadFirst: vi.fn(),
    }));
    try {
      const { nativeFlowPlanningPorts } = await import(
        '../../../modules/workflow/server/src/adapters/flow-planning-ports'
      );
      const db = { prepare: vi.fn() } as never;
      expect(await nativeFlowPlanningPorts(db).repository.readReadFlowRoute('company', 'leave', 'rec-2')).toBeNull();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});

describe('workflow flow-defaults/steps read tail (r42 native read plan)', () => {
  it('carries no retired owner SQL for the switched reads and keeps the r32 revision span', async () => {
    const source = await readFile(
      new URL('../../../modules/workflow/server/src/adapters/flow-planning-ports.ts', import.meta.url),
      'utf8',
    );
    expect(source).not.toContain('SELECT * FROM flow_defaults WHERE company_id=?');
    expect(source).not.toContain('SELECT revision,step_key,primary_user_id FROM flow_defaults');
    expect(source).not.toContain('SELECT steps_json FROM flow_routes WHERE company_id=?');
    expect(source).not.toContain('${flowDefaultsMaxRevisionSql} revision FROM flow_defaults');
    // Kept for guard25 (r24-r41 untouched); only the read call site was retired.
    expect(source).toContain('const flowDefaultsMaxRevisionSql = "SELECT COALESCE(MAX(revision),0)";');
    expect(source).toContain('(${flowDefaultsMaxRevisionSql} FROM flow_defaults WHERE company_id=?)=?');
  });

  it('plans the flow-defaults list/revision reads through the mounted native read with no local SQL', async () => {
    const rows = [{ revision: 4, step_key: 'draft', primary_user_id: 'actor' }];
    const all = vi.fn(async () => rows);
    const first = vi.fn(async () => ({ revision: 7 }));
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowReadFirst: first,
    }));
    try {
      const { nativeFlowPlanningPorts } = await import(
        '../../../modules/workflow/server/src/adapters/flow-planning-ports'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const repo = nativeFlowPlanningPorts(db).repository;
      expect(await repo.readSharedFlowReferencesRoutesdefaultsevents2('company')).toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith('wf_native_flow_defaults_by_company', ['company'], 'company', db);
      expect(await repo.readReadFlowDefaultsRevision('company')).toEqual({ revision: 7 });
      expect(first).toHaveBeenCalledWith('wf_native_flow_defaults_revision', ['company'], 'company', db);
      expect(await repo.readReadFlowDefaults('company', 'expense', null, null, 7)).toEqual({ results: rows });
      expect(all).toHaveBeenCalledWith(
        'wf_native_flow_defaults_by_kind_revision',
        ['company', 'expense', null, null, 7],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });

  it('plans the steps_json point read through the native read and keeps first-row parity', async () => {
    const first = vi.fn(async (): Promise<{ steps_json: string } | null> => ({ steps_json: '{}' }));
    const all = vi.fn();
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowReadFirst: first,
    }));
    try {
      const { nativeFlowPlanningPorts } = await import(
        '../../../modules/workflow/server/src/adapters/flow-planning-ports'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const repo = nativeFlowPlanningPorts(db).repository;
      expect(await repo.readAssignFlowPrimaryExisting('company', 'expense', 'record-1')).toEqual({ steps_json: '{}' });
      expect(first).toHaveBeenCalledWith(
        'wf_native_flow_routes_steps_json',
        ['company', 'expense', 'record-1'],
        'company',
        db,
      );
      first.mockResolvedValueOnce(null);
      expect(await repo.readAssignFlowPrimaryExisting('company', 'leave', 'record-2')).toBeNull();
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });
});
