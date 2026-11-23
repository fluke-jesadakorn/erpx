import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeCrmActivityWorkspaceClient,
  nativeCrmActivityWorkspaceConfiguration,
  NativeCrmActivityWorkspaceError,
} from '../../../packages/server-crm/src/adapters/native-activity-workspace';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44997',
  timeoutMs: 1000,
  holdings: [
    {
      origin: 'https://acme.example.test',
      holdingSlug: 'acme',
      holdingId: 'h001',
      companyId: 'company',
    },
  ],
});
const incoming = () =>
  new Headers({
    host: 'acme.example.test',
    cookie: '__Host-erpx-product=session',
  });
const actor = {
  userId: 'user',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment',
  role: 'owner',
  assignmentScope: 'holding',
  scope: { kind: 'holding' },
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['crm'],
  iam: { permissions: ['commerce.read', 'commerce.sales'] },
} as any;
const context = {
  userId: 'user',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  branchSlug: null,
  departmentSlug: null,
  assignmentId: 'assignment',
  assignmentVersion: 3,
  revision: 7,
  role: 'owner',
  assignmentScopeKind: 'holding',
  modules: ['crm'],
  permissions: ['commerce.read', 'commerce.sales'],
  expiresAt: 9_007_199_254_740_991,
};
const row = {
  id: 'activity-1',
  company_id: 'company',
  lead_id: 'lead-1',
  activity_type: 'call',
  subject: 'Call the customer',
  details: 'Discuss scope',
  owner_id: 'user',
  due_date: '2026-11-20',
  status: 'open',
  outcome: null,
  version: 1,
  created_by: 'user',
  created_at: 1,
  updated_at: 2,
  lead_title: 'Lead',
  lead_status: 'open',
  lead_stage: null,
  branch_id: 'branch',
  department_id: null,
  owner_name: 'User',
  owner_eligible: 1,
  ownerEligible: true,
  canComplete: true,
  canManage: false,
  canChange: true,
};
const workspaceData = {
  lead: { id: 'lead-1', title: 'Lead', status: 'open', stage: null, version: 1 },
  items: [row],
  canCreate: true,
  canManage: false,
  history: [{ action: 'create', version: 1 }],
  recipients: [{ id: 'user', name: 'User' }],
  purchase: { newWork: true, settleExisting: true, history: true, revision: '7' },
};
const workData = [
  {
    id: 'activity-1',
    type: 'crm_activity',
    title: 'CRM activity · Call the customer',
    detail: 'Discuss scope',
    href: '/crm/lead-1#crm-activities',
    status: 'open',
    updatedAt: 2,
    queue: 'todo',
    responsibleQueue: 'User',
    nextAction: 'Call the customer',
    dueAt: 1_795_132_800_000,
    blocker: null,
    relationships: ['assigned'],
  },
];
const ok = (data: unknown) =>
  new Response(JSON.stringify({ data, context }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const client = (transport: any) =>
  createNativeCrmActivityWorkspaceClient(nativeCrmActivityWorkspaceConfiguration(config), transport);

describe('native CRM activity workspace strict client', () => {
  it('forwards the registered Holding, session and CAS context and validates workspace', async () => {
    const transport = vi.fn().mockResolvedValue(ok(workspaceData));
    const result = await client(transport).workspace(incoming(), actor, 'lead-1');
    expect(result.lead.id).toBe('lead-1');
    expect(result.items[0].activity_type).toBe('call');
    expect(result.purchase?.revision).toBe('7');
    const [url, headers] = transport.mock.calls[0] as [URL, Headers];
    expect(url.pathname).toBe('/api/v1/holdings/acme/crm/activities');
    expect(url.searchParams.get('leadId')).toBe('lead-1');
    expect(headers.get('host')).toBe('acme.example.test');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
  });

  it('requests the work route without query and accepts optional canAct', async () => {
    const transport = vi.fn().mockResolvedValue(ok([{ ...workData[0], canAct: false }]));
    const result = await client(transport).work(incoming(), actor);
    const [url] = transport.mock.calls[0] as [URL, Headers];
    expect(url.pathname).toBe('/api/v1/holdings/acme/crm/activities/work');
    expect(url.search).toBe('');
    expect(result[0].canAct).toBe(false);
  });

  it.each([
    ['extra item field', { ...workspaceData, items: [{ ...row, unexpected: 1 }] }],
    ['missing recipients', { ...workspaceData, recipients: undefined }],
    ['wrong lead id', { ...workspaceData, lead: { ...workspaceData.lead, id: 'other' } }],
    ['cross-company row', { ...workspaceData, items: [{ ...row, company_id: 'other' }] }],
  ])('rejects %s without falling back', async (_name, data) => {
    const transport = vi.fn().mockResolvedValue(ok(data));
    await expect(client(transport).workspace(incoming(), actor, 'lead-1')).rejects.toBeInstanceOf(
      NativeCrmActivityWorkspaceError,
    );
  });

  it('rejects a changed context and a non-200 reply', async () => {
    const changed = ok(workspaceData);
    const wrong = new Response(JSON.stringify({ data: workspaceData, context: { ...context, revision: 9 } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
    await expect(
      client(() => Promise.resolve(wrong)).workspace(incoming(), actor, 'lead-1'),
    ).rejects.toMatchObject({ code: 'NATIVE_CRM_ACTIVITY_CONTEXT_CHANGED' });
    await expect(
      client(() => Promise.resolve(new Response('{}', { status: 503 }))).work(incoming(), actor),
    ).rejects.toBeInstanceOf(NativeCrmActivityWorkspaceError);
    void changed;
  });

  it('fails closed on invalid configuration', () => {
    const reservedPort = nativeCrmActivityWorkspaceConfiguration(
      JSON.stringify({ ...JSON.parse(config), privateBaseUrl: 'http://127.0.0.1:3100' }),
    );
    expect(() => createNativeCrmActivityWorkspaceClient(reservedPort)).toThrow(
      NativeCrmActivityWorkspaceError,
    );
    expect(() =>
      nativeCrmActivityWorkspaceConfiguration(JSON.stringify({ privateBaseUrl: 'nope' })),
    ).toThrow(NativeCrmActivityWorkspaceError);
  });
});
