import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  createNativeOrganizationClient,
  NativeOrganizationError,
  type NativeOrganizationPageContext,
} from '../../../modules/organization/ui/src/adapters/server/native-organization';
import { nativeOrganizationDraftRead } from '@erpx/organization-ui/lib/native-organization';

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
  iam: { permissions: ['people.manage', 'organization.manage'] },
};

const incoming = new Headers({ host: 'acme.example.test', cookie: 'erpx_session=current' });

const configuration = {
  privateBaseUrl: 'http://127.0.0.1:3999',
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
  timeoutMs: 1_000,
} as const;

const complete = {
  data: {
    draft: {
      id: 'draft-1',
      company_id: 'company',
      owner_id: 'user-1',
      state: 'draft',
      version: 2,
      definition: { items: [], reason: 'Move', effectiveAt: 1, timezone: 'Asia/Bangkok', managers: {}, leave: {} },
      created_at: 1,
      updated_at: 2,
      base_snapshot: { nodes: [], people: [] },
    },
    requests: [
      {
        id: 'request-1',
        item_keys: ['person:eng-1', 'position:pos-1'],
        state: 'scheduled',
        last_error: null,
        change_id: null,
      },
    ],
    recent: [{ id: 'draft-0', updated_at: 1, count: 3 }],
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

describe('native organization/drafts read client', () => {
  it('accepts the complete owner payload field-for-field and routes by Holding', async () => {
    const { api, calls } = client(complete);
    const result = await api.drafts(incoming, actor);
    expect(result).toEqual(complete.data);
    expect((result.draft?.definition as Record<string, unknown> | undefined)?.reason).toBe('Move');
    expect(result.requests[0]?.item_keys).toEqual(['person:eng-1', 'position:pos-1']);
    expect(result.recent[0]?.count).toBe(3);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.method).toBe('GET');
    expect(calls[0]?.url).toBe(
      'http://127.0.0.1:3999/api/v1/holdings/acme/people/organization/drafts',
    );
    expect(calls[0]?.headers.get('x-erpx-context-company')).toBe('company');
    expect(calls[0]?.headers.get('x-erpx-context-user')).toBe('user-1');
    expect(calls[0]?.headers.get('x-erpx-context-permissions')).toBe(
      'people.manage,organization.manage',
    );
    expect(calls[0]?.headers.get('x-erpx-context-scope')).toBe('holding');
  });

  it('sends a bounded id selector and rejects a malformed one locally', async () => {
    const { api, calls } = client(complete);
    await api.drafts(incoming, actor, 'draft-1');
    expect(calls[0]?.url).toContain('?id=draft-1');
    await expect(api.drafts(incoming, actor, 'bad id')).rejects.toBeInstanceOf(
      NativeOrganizationError,
    );
  });

  it('fails closed on a partial or malformed reply', async () => {
    const missingField = {
      data: { draft: complete.data.draft, requests: complete.data.requests },
    };
    await expect(client(missingField).api.drafts(incoming, actor)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID',
    });
    const badKeys = structuredClone(complete) as unknown as {
      data: { requests: Record<string, unknown>[] };
    };
    badKeys.data.requests[0]!.item_keys = 'person:eng-1';
    await expect(client(badKeys).api.drafts(incoming, actor)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID',
    });
    const badState = structuredClone(complete);
    badState.data.draft!.state = 'unknown';
    await expect(client(badState).api.drafts(incoming, actor)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_RESPONSE_INVALID',
    });
  });

  it('fails closed when a configured transport is unavailable', async () => {
    const transport = async () =>
      new Response(null, { status: 503, headers: { 'content-type': 'application/json' } });
    const api = createNativeOrganizationClient(configuration, transport);
    await expect(api.drafts(incoming, actor)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_UNAVAILABLE',
      status: 503,
    });
  });
});

describe('nativeOrganizationDraftRead route contract', () => {
  const original = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  afterEach(() => {
    if (original === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = original;
  });

  it('returns undefined without native configuration so the GET fails closed', async () => {
    delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    await expect(nativeOrganizationDraftRead(actor as never, 'acme', undefined, incoming)).resolves.toBeUndefined();
  });

  it('rejects a different Holding before a native request is made', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = JSON.stringify(configuration);
    await expect(nativeOrganizationDraftRead(actor as never, 'other', undefined, incoming)).rejects.toMatchObject({
      code: 'NATIVE_ORGANIZATION_CONTEXT_CHANGED',
    });
  });

  it('rejects a non-holding scope and missing draft permissions before a native request is made', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = JSON.stringify(configuration);
    await expect(
      nativeOrganizationDraftRead({ ...actor, scope: { kind: 'branch', branchSlug: 'branch-1' } } as never, 'acme', undefined, incoming),
    ).rejects.toMatchObject({ code: 'NATIVE_ORGANIZATION_AUTHORITY_INVALID' });
    await expect(
      nativeOrganizationDraftRead({ ...actor, iam: { permissions: ['people.manage'] } } as never, 'acme', undefined, incoming),
    ).rejects.toMatchObject({ code: 'NATIVE_ORGANIZATION_AUTHORITY_INVALID' });
  });
});

const routeSource = readFileSync(
  new URL(
    '../../../platform/workplace/src/routes/api/v1/holdings/[holdingSlug]/people/[...path]/route.ts',
    import.meta.url,
  ),
  'utf8',
);
const lines = routeSource.split('\n');

describe('organization/drafts catch-all cutover', () => {
  it('serves the exact GET branch from the native reader and fails closed with 503', () => {
    const branch = lines.find((line) =>
      line.includes('p[0]==="organization" && p[1]==="drafts"') &&
      line.includes('nativeOrganizationDraftRead('),
    );
    expect(branch).toBeDefined();
    expect(branch).toContain('nativeOrganizationDraftRead(');
    expect(branch).toContain('nativeOrganizationFailure(');
    expect(branch).toContain('503');
    expect(branch).not.toContain('organizationDraftApi(');
    expect(branch).not.toContain('getOrganizationDraft');
    expect(branch).not.toMatch(/\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE\b/);
    const nativeImport = lines.find((line) =>
      line.includes('@erpx/organization-ui/lib/native-organization'),
    );
    expect(nativeImport).toContain('nativeOrganizationDraftRead');
  });

  it('keeps the TS draft read only as a command oracle, never as a GET fallback', () => {
    const post = lines.find((line) =>
      line.includes('p[1]==="drafts"') && line.includes('organizationDraftApi('),
    );
    expect(post).toBeDefined();
    expect(post).toContain('organizationDraftApi(');
    const exported = readFileSync(
      new URL(
        '../../../modules/organization/server/src/public/organization-drafts.ts',
        import.meta.url,
      ),
      'utf8',
    );
    expect(exported).toContain('getOrganizationDraft');
  });
});
