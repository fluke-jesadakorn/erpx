import { describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  createNativeOrganizationClient,
  NativeOrganizationError,
  type NativeOrganizationPageContext,
} from '../../../packages/feature-organization/src/adapters/server/native-organization';
import {
  nativeOrganizationFailure,
  nativeOrganizationPositionSave,
  nativeOrganizationSelector,
  nativePositionSelector,
} from '@erpx/feature-organization/lib/native-organization';

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
  iam: { permissions: ['organization.manage'] },
};

const incoming = new Headers({
  host: 'acme.example.test',
  cookie: 'erpx_session=current',
});

const configuration = {
  privateBaseUrl: 'http://127.0.0.1:3999',
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
  timeoutMs: 1_000,
} as const;

type Call = {
  method: string;
  url: string;
  headers: Headers;
  body: string | undefined;
};

function client(response: unknown) {
  const calls: Call[] = [];
  const transport = async (
    method: 'GET' | 'POST',
    url: URL,
    headers: Headers,
    body: string | undefined,
  ) => {
    calls.push({ method, url: url.toString(), headers, body });
    return new Response(JSON.stringify(response), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  };
  return { calls, api: createNativeOrganizationClient(configuration, transport) };
}

describe('nativeOrganizationSelector', () => {
  it('selects only the exact owner command paths', () => {
    expect(nativeOrganizationSelector(['organization', 'levels'])).toEqual({ kind: 'levels' });
    expect(nativeOrganizationSelector(['organization', 'templates'])).toEqual({
      kind: 'templateSave',
      id: null,
    });
    expect(nativeOrganizationSelector(['organization', 'templates', 'template-1'])).toEqual({
      kind: 'templateSave',
      id: 'template-1',
    });
    expect(
      nativeOrganizationSelector(['organization', 'templates', 'template-1', 'apply']),
    ).toEqual({ kind: 'templateApply', id: 'template-1' });
    expect(nativeOrganizationSelector(['organization', 'changes', 'preview'])).toEqual({
      kind: 'preview',
    });
    expect(nativeOrganizationSelector(['organization', 'changes', 'id', 'review'])).toEqual({
      kind: 'review',
      id: 'id',
    });
    expect(nativeOrganizationSelector(['organization', 'drafts'])).toBeUndefined();
    expect(nativeOrganizationSelector(['organization', 'templates', 'id', 'other'])).toBeUndefined();
    expect(nativeOrganizationSelector(['organization', 'unknown'])).toBeUndefined();
  });
});

describe('native organization setup transport', () => {
  it('posts the fixed level catalog command and validates the exact receipt', async () => {
    const { api, calls } = client({ data: { id: 'company' } });
    const result = await api.change(incoming, actor, {
      path: '/levels',
      body: {},
      idempotencyKey: 'organization-levels-0001',
      result: 'levels',
    });
    expect(result).toEqual({ id: 'company' });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.method).toBe('POST');
    expect(calls[0]!.url).toBe(
      'http://127.0.0.1:3999/api/v1/holdings/acme/people/organization/levels',
    );
    expect(calls[0]!.body).toBe('{}');
    expect(calls[0]!.headers.get('idempotency-key')).toBe('organization-levels-0001');
    expect(calls[0]!.headers.get('cookie')).toBe('erpx_session=current');
    expect(calls[0]!.headers.get('x-erpx-context-company')).toBe('company');
  });

  it('uses one path for create and update but keeps the exact template receipt', async () => {
    const created = client({ data: { id: 'org-template-key', version: 1 } });
    await expect(
      created.api.change(incoming, actor, {
        path: '/templates',
        body: { name: 'Standard', positions: [], expectedVersion: 0 },
        idempotencyKey: 'organization-template-0001',
        result: 'template',
      }),
    ).resolves.toEqual({ id: 'org-template-key', version: 1 });
    expect(created.calls[0]!.url).toBe(
      'http://127.0.0.1:3999/api/v1/holdings/acme/people/organization/templates',
    );

    const updated = client({ data: { id: 'org-template-key', version: 2 } });
    await updated.api.change(incoming, actor, {
      path: '/templates/org-template-key',
      body: { name: 'Standard', positions: [], expectedVersion: 1 },
      idempotencyKey: 'organization-template-0002',
      result: 'template',
    });
    expect(updated.calls[0]!.url).toBe(
      'http://127.0.0.1:3999/api/v1/holdings/acme/people/organization/templates/org-template-key',
    );
  });

  it('posts template application and returns only bounded position identifiers', async () => {
    const { api, calls } = client({
      data: { id: 'org-template-key', positionIds: ['position-a', 'position-b'] },
    });
    const result = await api.change(incoming, actor, {
      path: '/templates/org-template-key/apply',
      body: {
        expectedVersion: 1,
        branchId: 'branch',
        departmentId: null,
        reason: 'Apply the reviewed template',
      },
      idempotencyKey: 'organization-template-apply-0001',
      result: 'apply',
    });
    expect(result).toEqual({ id: 'org-template-key', positionIds: ['position-a', 'position-b'] });
    expect(calls[0]!.url).toBe(
      'http://127.0.0.1:3999/api/v1/holdings/acme/people/organization/templates/org-template-key/apply',
    );
  });

  it('rejects malformed or foreign receipts instead of accepting a partial reply', async () => {
    const invalidLevels = client({ data: { id: 'company', extra: true } });
    await expect(
      invalidLevels.api.change(incoming, actor, {
        path: '/levels',
        body: {},
        idempotencyKey: 'organization-levels-0002',
        result: 'levels',
      }),
    ).rejects.toMatchObject({ name: 'NativeOrganizationError', code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID' });

    const invalidApply = client({ data: { id: 'org-template-key', positionIds: [''] } });
    await expect(
      invalidApply.api.change(incoming, actor, {
        path: '/templates/org-template-key/apply',
        body: { expectedVersion: 1, branchId: null, departmentId: null, reason: 'Apply it' },
        idempotencyKey: 'organization-template-apply-0002',
        result: 'apply',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID' });
  });

  it('refuses a host or holding context that does not match the configured owner', async () => {
    const { api, calls } = client({ data: { id: 'company' } });
    await expect(
      api.change(new Headers({ host: 'other.example.test' }), actor, {
        path: '/levels',
        body: {},
        idempotencyKey: 'organization-levels-0003',
        result: 'levels',
      }),
    ).rejects.toBeInstanceOf(NativeOrganizationError);
    expect(calls).toHaveLength(0);
    expect(nativeOrganizationFailure(new NativeOrganizationError('NATIVE_ORGANIZATION_CONTEXT_CHANGED'))).toEqual({
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before submitting.',
    });
  });
});

describe('native position selector and transport', () => {
  it('selects only position create and edit paths', () => {
    expect(nativePositionSelector(['organization', 'positions'])).toEqual({ id: null });
    expect(nativePositionSelector(['organization', 'positions', 'position-1'])).toEqual({
      id: 'position-1',
    });
    expect(nativePositionSelector(['organization', 'positions', 'position-1', 'default-pay'])).toBeUndefined();
    expect(nativePositionSelector(['organization', 'position-moves'])).toBeUndefined();
  });

  it('posts the exact position body and validates the command receipt', async () => {
    const { api, calls } = client({ data: { id: 'position-position-key', version: 1 } });
    const body = {
      title: 'Operations Lead',
      branchId: null,
      departmentId: null,
      levelId: 'level-3',
      reportsToPositionId: 'position-ceo',
      isCeo: false,
      expectedVersion: 0,
      reason: 'Open the operations lead position',
    };
    const result = await api.change(incoming, actor, {
      path: '/positions',
      body,
      idempotencyKey: 'position-key-0001',
      result: 'position',
    });
    expect(result).toEqual({ id: 'position-position-key', version: 1 });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url).toBe(
      'http://127.0.0.1:3999/api/v1/holdings/acme/people/organization/positions',
    );
    expect(calls[0]!.body).toBe(JSON.stringify(body));
    expect(calls[0]!.headers.get('idempotency-key')).toBe('position-key-0001');
  });

  it('fails closed without a native configuration', async () => {
    const previous = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    try {
      await expect(
        nativeOrganizationPositionSave(
          { ...actor, modules: [], iam: { permissions: [] } } as never,
          'acme',
          ['organization', 'positions'],
          {},
          'position-key-0002',
          incoming,
        ),
      ).resolves.toBeUndefined();
    } finally {
      if (previous !== undefined) process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = previous;
    }
  });
});
