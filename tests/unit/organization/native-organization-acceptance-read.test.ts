import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  createNativeOrganizationClient,
  NativeOrganizationError,
  type NativeOrganizationPageContext,
} from '../../../packages/feature-organization/src/adapters/server/native-organization';
import { nativeOrganizationAcceptance } from '@erpx/feature-organization/lib/native-organization';

const actor: NativeOrganizationPageContext = {
  userId: 'user-1',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-1',
  role: 'holding_owner',
  assignmentScope: 'holding',
  scope: { kind: 'holding' },
  authority: { revision: 7, assignmentVersion: 1 },
  modules: ['people'],
  iam: { permissions: ['organization.manage'], personUserIds: ['person-user'] },
};

const incoming = new Headers({ host: 'acme.example.test', cookie: 'erpx_session=current' });
const configuration = {
  privateBaseUrl: 'http://127.0.0.1:3999',
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
  timeoutMs: 1_000,
} as const;

const complete = {
  data: {
    companyId: 'company',
    status: 'submitted',
    version: 3,
    checks: [
      { id: 'company-active', label: 'Company identity is active', ready: true, owner: 'Company administrator', detail: 'Acme' },
      { id: 'branch-ready', label: 'At least one active branch', ready: true, owner: 'Company administrator', detail: '2 active branch(es)' },
      { id: 'levels-ready', label: 'Six numbered staff levels', ready: true, owner: 'Company HR administrator', detail: 'Levels 1–6 are installed.' },
      { id: 'ceo-ready', label: 'Active CEO position', ready: true, owner: 'People administrator', detail: 'One active CEO position is configured.' },
      { id: 'staff-ready', label: 'Active staff have an organization placement', ready: true, owner: 'People manager', detail: 'All active staff have a level and position.' },
    ],
    blockers: [],
    canSubmit: false,
    canReview: true,
    canRevoke: false,
    requestedBy: 'requester-1',
    reviewedBy: null,
    reason: 'Ready for review',
    reviewReason: null,
    updatedAt: 12,
    events: [
      { id: 'event-1', action: 'submitted', actor_id: 'requester-1', version: 3, reason: 'Ready for review', created_at: 12 },
    ],
  },
};

type Call = { method: string; url: string; headers: Headers };
function client(response: unknown) {
  const calls: Call[] = [];
  const transport = async (
    method: 'GET' | 'POST',
    url: URL,
    headers: Headers,
    _body: string | undefined,
  ) => {
    calls.push({ method, url: url.toString(), headers });
    return new Response(JSON.stringify(response), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  };
  return { calls, api: createNativeOrganizationClient(configuration, transport) };
}

describe('native organization/acceptance read client', () => {
  it('accepts the complete owner payload field-for-field and routes by Holding', async () => {
    const { api, calls } = client(complete);
    const result = await api.acceptance(incoming, actor);
    expect(result).toEqual(complete.data);
    expect(result.checks).toHaveLength(5);
    expect(result.events[0]?.actor_id).toBe('requester-1');
    expect(calls).toHaveLength(1);
    expect(calls[0]?.method).toBe('GET');
    expect(calls[0]?.url).toBe(
      'http://127.0.0.1:3999/api/v1/holdings/acme/people/organization/acceptance',
    );
    expect(calls[0]?.headers.get('x-erpx-context-company')).toBe('company');
    expect(calls[0]?.headers.get('x-erpx-context-assignment')).toBe('assignment-1');
    expect(calls[0]?.headers.get('x-erpx-context-person-ids')).toBe('person-user');
    expect(calls[0]?.headers.get('x-erpx-context-scope')).toBe('holding');
  });

  it('fails closed on a partial or malformed reply', async () => {
    const missing = structuredClone(complete) as { data: Record<string, unknown> };
    delete missing.data.canReview;
    await expect(client(missing).api.acceptance(incoming, actor)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID',
    });
    const badCheck = structuredClone(complete) as {
      data: { checks: Record<string, unknown>[] };
    };
    badCheck.data.checks[0]!.ready = 'yes';
    await expect(client(badCheck).api.acceptance(incoming, actor)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID',
    });
    const badStatus = structuredClone(complete) as { data: { status: string } };
    badStatus.data.status = 'unknown';
    await expect(client(badStatus).api.acceptance(incoming, actor)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID',
    });
  });

  it('fails closed when a configured transport is unavailable', async () => {
    const transport = async () =>
      new Response(null, { status: 503, headers: { 'content-type': 'application/json' } });
    await expect(
      createNativeOrganizationClient(configuration, transport).acceptance(incoming, actor),
    ).rejects.toMatchObject({ code: 'NATIVE_ORGANIZATION_UNAVAILABLE', status: 503 });
  });
});

describe('nativeOrganizationAcceptance route contract', () => {
  const original = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  afterEach(() => {
    if (original === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = original;
  });

  it('returns undefined without native configuration so the GET fails closed', async () => {
    delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    await expect(
      nativeOrganizationAcceptance(actor as never, 'acme', incoming),
    ).resolves.toBeUndefined();
  });

  it('rejects a different Holding or a narrow scope before a native request', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = JSON.stringify(configuration);
    await expect(
      nativeOrganizationAcceptance(actor as never, 'other', incoming),
    ).rejects.toMatchObject({ code: 'NATIVE_ORGANIZATION_CONTEXT_CHANGED' });
    await expect(
      nativeOrganizationAcceptance(
        { ...actor, scope: { kind: 'branch', branchSlug: 'branch-1' } } as never,
        'acme',
        incoming,
      ),
    ).rejects.toBeInstanceOf(NativeOrganizationError);
  });
});
