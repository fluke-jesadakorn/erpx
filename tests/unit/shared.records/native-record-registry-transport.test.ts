import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeRecordRegistryClient,
  nativeRecordRegistryAvailable,
  nativeRecordRegistryConfiguration,
  nativeRecordRegistryFailure,
  NativeRecordRegistryError,
  type NativeRecordRegistryTransport,
} from '../../../packages/server-records/src/public/native-record-registry';
import type {
  NativeRecordView,
  NativeRecordsPageContext,
} from '../../../packages/server-records/src/public/native-record-context';

const config = (overrides: Record<string, unknown> = {}) => ({
  privateBaseUrl: 'http://127.0.0.1:44994',
  timeoutMs: 1000,
  holdings: [
    { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
  ],
  ...overrides,
});

const actor: NativeRecordsPageContext = {
  userId: 'actor',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-actor',
  role: 'operations_manager',
  assignmentScope: 'holding',
  modules: ['operations', 'crm'],
  authority: { revision: 9, assignmentVersion: 3 },
  iam: { permissions: ['records.read', 'records.review'] },
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
  role: 'operations_manager',
  scopeKind: 'holding',
  modules: ['operations', 'crm'],
  permissions: ['records.read', 'records.review'],
  expiresAt: 9_007_199_254_740_991,
};

const view = (id: string, updatedAt: number, moduleId = 'operations', sectionId = 'overview'): NativeRecordView => ({
  moduleId,
  id,
  sectionId,
  title: `Record ${id}`,
  description: 'Bounded registry row',
  status: 'draft',
  fields: [{ key: 'note', label: 'Note', type: 'text', value: 'ok' }],
  version: 1,
  createdBy: 'maker',
  updatedAt,
});

const cursor = (key: number, id: string, reverse = false) =>
  Buffer.from(
    JSON.stringify({ key: String(key), id, reverse, scope: '["records.registry.v1"]' }),
  ).toString('base64url');

const page = {
  items: [view('rec-2', 500), view('rec-1', 400)],
  nextCursor: cursor(400, 'rec-1'),
  previousCursor: null,
  hasMore: true,
};

const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.com',
    cookie: '__Host-erpx-product=session',
    ...overrides,
  });

const pageInput = {
  holdingSlug: 'acme',
  moduleId: 'operations',
  sectionId: 'overview',
  q: 'needle review',
  cursor: cursor(600, 'rec-3'),
};
const listInput = { holdingSlug: 'acme', moduleId: 'operations', sectionId: 'overview' };

function client(body: unknown = { data: page, context }, status = 200) {
  const calls: { url: URL; headers: Headers }[] = [];
  const transport: NativeRecordRegistryTransport = async (method, url, headers) => {
    calls.push({ url, headers });
    expect(method).toBe('GET');
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    });
  };
  return {
    calls,
    client: createNativeRecordRegistryClient(config() as never, transport),
  };
}

describe('native Records registry transport', () => {
  it('validates configuration closed', () => {
    expect(nativeRecordRegistryAvailable()).toBe(
      process.env.ERPX_NATIVE_RECORDS_CONFIG !== undefined,
    );
    expect(nativeRecordRegistryConfiguration(JSON.stringify(config()))).toEqual(
      config() as never,
    );
    const rejects = (run: () => unknown) => {
      try {
        run();
      } catch (error) {
        expect(error).toBeInstanceOf(NativeRecordRegistryError);
        expect(error).toMatchObject({
          code: 'NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID',
        });
        return;
      }
      throw new Error('expected configuration failure');
    };
    for (const value of [
      config({ privateBaseUrl: 'http://127.0.0.1:3100' }),
      config({ privateBaseUrl: 'http://127.0.0.1:3299' }),
      config({ privateBaseUrl: 'http://localhost:44994' }),
      config({ privateBaseUrl: 'https://127.0.0.1:44994' }),
      config({ privateBaseUrl: 'http://127.0.0.1:44994/path' }),
      config({ timeoutMs: 249 }),
      config({ timeoutMs: 30_001 }),
      config({ holdings: [] }),
      config({
        holdings: [
          { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
          { origin: 'https://acme.example.com', holdingSlug: 'other', holdingId: 'h002' },
        ],
      }),
      config({
        holdings: [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
      }),
    ])
      rejects(() => createNativeRecordRegistryClient(value as never));
    rejects(() =>
      nativeRecordRegistryConfiguration(JSON.stringify({ ...config(), extra: true })),
    );
    rejects(() => nativeRecordRegistryConfiguration('{'));
  });

  it('forwards exact paged and unpaged registry reads', async () => {
    const { calls, client: reader } = client();
    const value = await reader.page(incoming(), actor, pageInput);
    expect(value.items).toHaveLength(2);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/records/operations?section=overview&paged=1&q=needle+review&cursor=' +
        encodeURIComponent(pageInput.cursor),
    );
    expect(calls[0]!.headers.get('host')).toBe('acme.example.com');
    expect(calls[0]!.headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(calls[0]!.headers.get('accept')).toBe('application/json');

    const listedClient = client({ data: page.items, context });
    const listed = await listedClient.client.list(incoming(), actor, listInput);
    expect(listed).toHaveLength(2);
    expect(listedClient.calls[0]!.url.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/records/operations?section=overview',
    );
  });

  it('rejects wrong host, changed context and unbounded inputs before transport', async () => {
    for (const [expected, overrides, nextActor, nextInput, run] of [
      [
        'NATIVE_RECORD_REGISTRY_AUTHORITY_INVALID',
        { host: 'other.example.com' },
        actor,
        pageInput,
        'page',
      ],
      [
        'NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED',
        {},
        { ...actor, holdingSlug: 'other' },
        pageInput,
        'page',
      ],
      [
        'NATIVE_RECORD_REGISTRY_REQUEST_INVALID',
        {},
        actor,
        { ...pageInput, moduleId: '../operations' },
        'page',
      ],
      [
        'NATIVE_RECORD_REGISTRY_REQUEST_INVALID',
        {},
        actor,
        { ...pageInput, sectionId: ' overview' },
        'page',
      ],
      [
        'NATIVE_RECORD_REGISTRY_REQUEST_INVALID',
        {},
        actor,
        { ...pageInput, q: 'x'.repeat(201) },
        'page',
      ],
      [
        'NATIVE_RECORD_REGISTRY_REQUEST_INVALID',
        {},
        actor,
        { ...pageInput, cursor: 'not-a-cursor' },
        'page',
      ],
      [
        'NATIVE_RECORD_REGISTRY_REQUEST_INVALID',
        {},
        actor,
        { ...listInput, q: 'needle' },
        'list',
      ],
    ] as const) {
      const { calls, client: reader } = client();
      await expect(
        run === 'page'
          ? reader.page(incoming(overrides), nextActor, nextInput as typeof pageInput)
          : reader.list(incoming(overrides), nextActor, nextInput as typeof listInput),
      ).rejects.toMatchObject({ code: expected });
      expect(calls).toHaveLength(0);
    }
  });

  it('rejects foreign, duplicate, unordered, cursor and coherence failures', async () => {
    const mutations: unknown[] = [
      { ...page, items: [{ ...page.items[0], moduleId: 'crm' }], hasMore: false, nextCursor: null },
      { ...page, items: [{ ...page.items[0], sectionId: 'drafts' }], hasMore: false, nextCursor: null },
      { ...page, items: [page.items[0], page.items[0]] },
      {
        ...page,
        items: [view('rec-1', 400), view('rec-2', 500)],
        previousCursor: null,
        nextCursor: null,
        hasMore: false,
      },
      { ...page, nextCursor: 'not-a-cursor' },
      { ...page, hasMore: false },
      { ...page, extra: true },
    ];
    for (const value of mutations) {
      const { client: reader } = client({ data: value, context });
      await expect(reader.page(incoming(), actor, pageInput)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_REGISTRY_RESPONSE_INVALID',
      });
    }
    for (const value of [
      [view('rec-1', 400, 'crm')],
      [page.items[0], page.items[0]],
      [view('rec-1', 400), view('rec-2', 500)],
    ]) {
      const { client: reader } = client({ data: value, context });
      await expect(reader.list(incoming(), actor, listInput)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_REGISTRY_RESPONSE_INVALID',
      });
    }
    const changedContexts = [
      { ...context, companyId: 'foreign' },
      { ...context, permissions: ['records.read'] },
      { ...context, modules: ['crm'] },
      { ...context, revision: 10 },
    ];
    for (const value of changedContexts) {
      const { client: reader } = client({ data: page, context: value });
      await expect(reader.page(incoming(), actor, pageInput)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED',
      });
    }
    {
      const { client: reader } = client({ data: page, context: { ...context, extra: true } });
      await expect(reader.page(incoming(), actor, pageInput)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_REGISTRY_RESPONSE_INVALID',
      });
    }
  });

  it('maps owner failures without falling back', async () => {
    for (const [status, code] of [
      [404, 'NOT_FOUND'],
      [403, 'PERMISSION_REQUIRED'],
      [409, 'WORK_CONTEXT_CHANGED'],
      [422, 'RECORDS_REGISTRY_REQUEST_INVALID'],
      [422, 'INVALID_CURSOR'],
      [503, 'RECORDS_REGISTRY_SOURCE_INVALID'],
    ] as const) {
      const { client: reader } = client(
        { error: { code, message: 'Synthetic owner failure.' } },
        status,
      );
      await expect(reader.page(incoming(), actor, pageInput)).rejects.toMatchObject({
        code,
        status,
      });
      expect(nativeRecordRegistryFailure(new Error(code))).toBeUndefined();
      expect(nativeRecordRegistryFailure({ code })).toBeUndefined();
    }
    expect(
      nativeRecordRegistryFailure(
        new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_CONTEXT_CHANGED'),
      ),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(
      nativeRecordRegistryFailure(
        new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_CONFIGURATION_INVALID'),
      ),
    ).toMatchObject({ status: 503, code: 'NATIVE_RECORD_REGISTRY_UNAVAILABLE' });
    expect(
      nativeRecordRegistryFailure(
        new NativeRecordRegistryError('NATIVE_RECORD_REGISTRY_REQUEST_INVALID'),
      ),
    ).toMatchObject({ status: 422, code: 'RECORDS_REGISTRY_REQUEST_INVALID' });
  });
  it('posts a bounded create with origin and idempotency and returns the created id', async () => {
    const calls: { method: string; url: URL; headers: Headers; body: string | undefined }[] = [];
    const createdView = view('rec-support-records:http-1', 1, 'support', 'tickets');
    const transport: NativeRecordRegistryTransport = async (method, url, headers, body) => {
      calls.push({ method, url, headers, body });
      return new Response(JSON.stringify({ data: createdView, context }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    };
    const writer = createNativeRecordRegistryClient(config() as never, transport);
    const input = {
      holdingSlug: 'acme',
      moduleId: 'support',
      sectionId: 'tickets',
      title: 'Native record',
      description: '',
      fields: [{ key: 'priority', label: 'Priority', type: 'text', value: 'high' as const }],
    };
    const created = await writer.create(
      incoming({ origin: 'https://acme.example.com' }),
      actor,
      input,
      'records:http-1',
    );
    expect(created).toEqual(view('rec-support-records:http-1', 1, 'support', 'tickets'));
    expect(calls).toHaveLength(1);
    expect(calls[0]!.method).toBe('POST');
    expect(calls[0]!.url.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/records/support',
    );
    expect(calls[0]!.headers.get('origin')).toBe('https://acme.example.com');
    expect(calls[0]!.headers.get('idempotency-key')).toBe('records:http-1');
    expect(calls[0]!.headers.get('content-type')).toBe('application/json');
    expect(calls[0]!.headers.get('x-erpx-context-company')).toBe('company');
    expect(calls[0]!.headers.get('x-erpx-context-assignment')).toBe('assignment-actor');
    expect(calls[0]!.headers.get('x-erpx-context-revision')).toBe('9');
    expect(calls[0]!.headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(calls[0]!.headers.get('x-erpx-workspace-branch')).toBeNull();
    expect(calls[0]!.headers.get('x-erpx-workspace-department')).toBeNull();
    expect(calls[0]!.headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(JSON.parse(calls[0]!.body!)).toEqual({
      sectionId: 'tickets',
      title: 'Native record',
      description: '',
      fields: input.fields,
    });
  });

  it('rejects missing origin, bad keys and duplicate fields before transport', async () => {
    const calls: string[] = [];
    const transport: NativeRecordRegistryTransport = async (method) => {
      calls.push(method);
      return new Response('{}', { status: 200 });
    };
    const writer = createNativeRecordRegistryClient(config() as never, transport);
    const base = {
      holdingSlug: 'acme',
      moduleId: 'support',
      sectionId: 'tickets',
      title: 'Native record',
      description: '',
      fields: [{ key: 'priority', label: 'Priority', type: 'text', value: 'high' as const }],
    };
    await expect(
      writer.create(incoming(), actor, base, 'records:http-1'),
    ).rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN' });
    await expect(
      writer.create(incoming({ origin: 'https://acme.example.com' }), actor, base, 'short'),
    ).rejects.toMatchObject({ code: 'IDEMPOTENCY_KEY_REQUIRED' });
    await expect(
      writer.create(incoming({ origin: 'https://acme.example.com' }), actor, {
        ...base,
        fields: [base.fields[0]!, base.fields[0]!],
      }, 'records:http-2'),
    ).rejects.toMatchObject({ code: 'NATIVE_RECORD_REGISTRY_REQUEST_INVALID' });
    expect(calls).toEqual([]);
  });

});
