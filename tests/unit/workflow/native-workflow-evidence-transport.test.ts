import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeWorkflowEvidenceClient,
  nativeWorkflowEvidenceConfiguration,
  nativeWorkflowEvidenceFailure,
  NativeWorkflowEvidenceError,
  type NativeWorkflowEvidenceActor,
  type NativeWorkflowEvidenceConfiguration,
} from '../../../packages/server-workflow/src/public/native-workflow-evidence';

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
