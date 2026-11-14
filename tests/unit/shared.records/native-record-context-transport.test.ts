import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  createNativeRecordContextClient,
  nativeRecordContextAvailable,
  nativeRecordContextConfiguration,
  nativeRecordContextFailure,
  NativeRecordContextError,
  createNativeRecordDetailsClient,
  nativeRecordDetailsAvailable,
  nativeRecordDetailsConfiguration,
  nativeRecordDetailsFailure,
  NativeRecordDetailsError,
  type NativeRecordsPageContext,
  type NativeRecordContextTransport,
  type NativeRecordDetailsInput,
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

const data = {
  record: {
    moduleId: 'operations',
    id: 'rec-1',
    sectionId: 'overview',
    title: 'Service visit',
    description: 'Bounded record',
    status: 'draft',
    fields: [{ key: 'note', label: 'Note', type: 'text', value: 'first' }],
    version: 3,
    createdBy: 'maker',
    updatedAt: 500,
  },
  references: [
    {
      target_module: 'operations',
      target_id: 'rec-2',
      kind: 'dependsOn',
      ordinal: 0,
      title: 'Target record',
      version: 2,
      access: 'available',
    },
    {
      target_module: 'crm',
      target_id: 'party-1',
      kind: 'authoredWith',
      ordinal: 1,
      title: 'Unavailable record',
      version: 0,
      access: 'denied',
    },
  ],
  backlinks: [
    {
      host_module: 'operations',
      host_id: 'rec-3',
      kind: 'blocks',
      title: 'Host record',
      version: 3,
    },
  ],
  mentions: [
    { target_kind: 'person', target_id: 'user-1', target_label: 'Person One', created_at: 400 },
  ],
  suggestions: [
    {
      id: 'suggestion-1',
      field_key: 'note',
      proposed_value_json: '"next"',
      source_versions_json: '[{"record":"operations/rec-2","version":2}]',
      provenance_json: '{"provider_kind":"development"}',
      state: 'ready',
      created_at: 420,
    },
  ],
  history: [
    {
      from_status: 'draft',
      to_status: 'in_review',
      action: 'submit',
      note: 'Submitted',
      actor_id: 'maker',
      record_version: 2,
      created_at: 460,
    },
  ],
};

const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.com',
    cookie: '__Host-erpx-product=session',
    ...overrides,
  });

const input = { holdingSlug: 'acme', moduleId: 'operations', recordId: 'rec-1' };

function client(body: unknown = { data, context }, status = 200) {
  const calls: { url: URL; headers: Headers }[] = [];
  const transport: NativeRecordContextTransport = async (method, url, headers) => {
    calls.push({ url, headers });
    expect(method).toBe('GET');
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    });
  };
  return {
    calls,
    client: createNativeRecordContextClient(config() as never, transport),
  };
}

describe('native Records context transport', () => {
  it('validates configuration closed', () => {
    expect(nativeRecordContextAvailable()).toBe(
      process.env.ERPX_NATIVE_RECORDS_CONFIG !== undefined,
    );
    expect(nativeRecordContextConfiguration(JSON.stringify(config()))).toEqual(
      config() as never,
    );
    const rejectsConfiguration = (run: () => unknown) => {
      try {
        run();
      } catch (error) {
        expect(error).toBeInstanceOf(NativeRecordContextError);
        expect(error).toMatchObject({ code: 'NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID' });
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
      rejectsConfiguration(() => createNativeRecordContextClient(value as never));
    rejectsConfiguration(() =>
      nativeRecordContextConfiguration(JSON.stringify({ ...config(), extra: true })),
    );
    rejectsConfiguration(() => nativeRecordContextConfiguration('{'));
  });

  it('forwards the exact target, session and bounded record path', async () => {
    const { calls, client: reader } = client();
    const value = await reader.context(incoming(), actor, input);
    expect(value.record.id).toBe('rec-1');
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/records/operations/rec-1',
    );
    expect(calls[0]!.headers.get('host')).toBe('acme.example.com');
    expect(calls[0]!.headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(calls[0]!.headers.get('accept')).toBe('application/json');
  });

  it('rejects wrong host, changed context and unbounded identifiers before transport', async () => {
    for (const [expected, overrides, nextActor, nextInput] of [
      [
        'NATIVE_RECORD_CONTEXT_AUTHORITY_INVALID',
        { host: 'other.example.com' },
        actor,
        input,
      ],
      [
        'NATIVE_RECORD_CONTEXT_CONTEXT_CHANGED',
        {},
        { ...actor, holdingSlug: 'other' },
        input,
      ],
      [
        'NATIVE_RECORD_CONTEXT_REQUEST_INVALID',
        {},
        actor,
        { ...input, moduleId: '../operations' },
      ],
      [
        'NATIVE_RECORD_CONTEXT_REQUEST_INVALID',
        {},
        actor,
        { ...input, recordId: 'rec/1' },
      ],
    ] as const) {
      const { calls, client: reader } = client();
      await expect(
        reader.context(incoming(overrides), nextActor, nextInput as typeof input),
      ).rejects.toMatchObject({ code: expected });
      expect(calls).toHaveLength(0);
    }
  });

  it('rejects foreign, duplicate, extra and incoherent owner rows', async () => {
    const mutations: unknown[] = [
      { ...data, record: { ...data.record, id: 'rec-2' } },
      { ...data, record: { ...data.record, moduleId: 'crm' } },
      {
        ...data,
        references: [
          data.references[0],
          { ...data.references[1], target_id: 'rec-2', target_module: 'operations' },
        ],
      },
      {
        ...data,
        references: [
          { ...data.references[0], access: 'denied', title: 'Unavailable record', version: 2 },
        ],
      },
      { ...data, backlinks: [data.backlinks[0], data.backlinks[0]] },
      { ...data, suggestions: [data.suggestions[0], data.suggestions[0]] },
      { ...data, history: [{ ...data.history[0], action: 'delete' }] },
      { ...data, extra: true },
    ];
    for (const value of mutations) {
      const { client: reader } = client({ data: value, context });
      await expect(reader.context(incoming(), actor, input)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_CONTEXT_RESPONSE_INVALID',
      });
    }
    const changedContexts = [
      { ...context, companyId: 'foreign' },
      { ...context, permissions: ['records.read'] },
      { ...context, modules: ['crm'] },
      { ...context, revision: 10 },
    ];
    for (const value of changedContexts) {
      const { client: reader } = client({ data, context: value });
      await expect(reader.context(incoming(), actor, input)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_CONTEXT_CONTEXT_CHANGED',
      });
    }
    {
      const { client: reader } = client({ data, context: { ...context, extra: true } });
      await expect(reader.context(incoming(), actor, input)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_CONTEXT_RESPONSE_INVALID',
      });
    }
  });

  it('maps owner failures without falling back', async () => {
    for (const [status, code] of [
      [404, 'NOT_FOUND'],
      [403, 'PERMISSION_REQUIRED'],
      [409, 'WORK_CONTEXT_CHANGED'],
      [422, 'RECORD_CONTEXT_REQUEST_INVALID'],
      [503, 'RECORDS_CONTEXT_CONFIGURATION_INVALID'],
    ] as const) {
      const { client: reader } = client(
        { error: { code, message: 'Synthetic owner failure.' } },
        status,
      );
      await expect(reader.context(incoming(), actor, input)).rejects.toMatchObject({
        code,
        status,
      });
      expect(nativeRecordContextFailure(new Error(code))).toBeUndefined();
      expect(nativeRecordContextFailure({ code })).toBeUndefined();
    }
    expect(
      nativeRecordContextFailure(
        new NativeRecordContextError('NATIVE_RECORD_CONTEXT_CONTEXT_CHANGED'),
      ),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(
      nativeRecordContextFailure(
        new NativeRecordContextError('NATIVE_RECORD_CONTEXT_CONFIGURATION_INVALID'),
      ),
    ).toMatchObject({ status: 503, code: 'NATIVE_RECORD_CONTEXT_UNAVAILABLE' });
    expect(
      nativeRecordContextFailure(
        new NativeRecordContextError('NATIVE_RECORD_CONTEXT_REQUEST_INVALID'),
      ),
    ).toMatchObject({ status: 422, code: 'RECORD_CONTEXT_REQUEST_INVALID' });
  });
});


const detail = {
  kind: 'workspace_record' as const,
  row: data.record,
  related: [{ kind: 'operations', id: 'rec-2' }],
  history: data.history,
  page: 1,
  hasNext: false,
};
const detailInput: NativeRecordDetailsInput = {
  holdingSlug: 'acme',
  moduleId: 'operations',
  recordId: 'rec-1',
};

function detailClient(body: unknown = { data: detail, context }, status = 200) {
  const calls: { url: URL; headers: Headers }[] = [];
  const transport: NativeRecordContextTransport = async (method, url, headers) => {
    calls.push({ url, headers });
    expect(method).toBe('GET');
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    });
  };
  return {
    calls,
    client: createNativeRecordDetailsClient(config() as never, transport),
  };
}

describe('native Records detail transport', () => {
  it('validates configuration and availability closed', () => {
    expect(nativeRecordDetailsAvailable()).toBe(
      process.env.ERPX_NATIVE_RECORDS_CONFIG !== undefined,
    );
    expect(nativeRecordDetailsConfiguration(JSON.stringify(config()))).toEqual(
      config() as never,
    );
    for (const value of [
      config({ privateBaseUrl: 'http://127.0.0.1:3100' }),
      config({ timeoutMs: 249 }),
      config({ holdings: [] }),
    ])
      expect(() => createNativeRecordDetailsClient(value as never)).toThrow(
        NativeRecordDetailsError,
      );
  });

  it('forwards the exact target, session and bounded detail page', async () => {
    const { calls, client: reader } = detailClient({
      data: { ...detail, page: 2 },
      context,
    });
    const value = await reader.supplement(incoming(), actor, {
      ...detailInput,
      page: 2,
    });
    expect(value.row.id).toBe('rec-1');
    expect(calls).toHaveLength(1);
    expect(calls[0]!.url.toString()).toBe(
      'http://127.0.0.1:44994/api/v1/holdings/acme/records/operations/rec-1/detail?page=2',
    );
    expect(calls[0]!.headers.get('host')).toBe('acme.example.com');
    expect(calls[0]!.headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(calls[0]!.headers.get('accept')).toBe('application/json');
  });

  it('rejects wrong host, changed context and unbounded identifiers before transport', async () => {
    for (const [expected, overrides, nextActor, nextInput] of [
      [
        'NATIVE_RECORD_DETAILS_AUTHORITY_INVALID',
        { host: 'other.example.com' },
        actor,
        detailInput,
      ],
      [
        'NATIVE_RECORD_DETAILS_CONTEXT_CHANGED',
        {},
        { ...actor, holdingSlug: 'other' },
        detailInput,
      ],
      [
        'NATIVE_RECORD_DETAILS_REQUEST_INVALID',
        {},
        actor,
        { ...detailInput, moduleId: '../operations' },
      ],
      [
        'NATIVE_RECORD_DETAILS_REQUEST_INVALID',
        {},
        actor,
        { ...detailInput, recordId: 'rec/1' },
      ],
      [
        'NATIVE_RECORD_DETAILS_REQUEST_INVALID',
        {},
        actor,
        { ...detailInput, page: 0 },
      ],
      [
        'NATIVE_RECORD_DETAILS_REQUEST_INVALID',
        {},
        actor,
        { ...detailInput, page: 100_001 },
      ],
    ] as const) {
      const { calls, client: reader } = detailClient();
      await expect(
        reader.supplement(incoming(overrides), nextActor, nextInput as NativeRecordDetailsInput),
      ).rejects.toMatchObject({ code: expected });
      expect(calls).toHaveLength(0);
    }
  });

  it('rejects foreign, paginated and unbounded owner details', async () => {
    const mutations: unknown[] = [
      { ...detail, row: { ...detail.row, id: 'rec-2' } },
      { ...detail, row: { ...detail.row, moduleId: 'crm' } },
      { ...detail, page: 2 },
      { ...detail, kind: 'other' },
      { ...detail, extra: true },
      {
        ...detail,
        related: [detail.related[0], detail.related[0]],
      },
      {
        ...detail,
        related: Array.from({ length: 51 }, (_, index) => ({
          kind: 'operations',
          id: `rec-${index}`,
        })),
      },
      { ...detail, history: Array.from({ length: 21 }, () => data.history[0]) },
      { ...detail, history: [{ ...data.history[0], action: 'delete' }] },
      { ...detail, hasNext: 'yes' },
    ];
    for (const value of mutations) {
      const { client: reader } = detailClient({ data: value, context });
      await expect(reader.supplement(incoming(), actor, detailInput)).rejects.toMatchObject({
        code: 'NATIVE_RECORD_DETAILS_RESPONSE_INVALID',
      });
    }
    const { client: reader } = detailClient({
      data: detail,
      context: { ...context, companyId: 'foreign' },
    });
    await expect(reader.supplement(incoming(), actor, detailInput)).rejects.toMatchObject({
      code: 'NATIVE_RECORD_CONTEXT_CONTEXT_CHANGED',
    });
  });

  it('maps owner failures without falling back', async () => {
    for (const [status, code] of [
      [404, 'NOT_FOUND'],
      [403, 'PERMISSION_REQUIRED'],
      [409, 'WORK_CONTEXT_CHANGED'],
      [422, 'RECORD_DETAILS_PAGE_INVALID'],
      [503, 'RECORDS_DETAILS_CONFIGURATION_INVALID'],
    ] as const) {
      const { client: reader } = detailClient(
        { error: { code, message: 'Synthetic owner failure.' } },
        status,
      );
      await expect(reader.supplement(incoming(), actor, detailInput)).rejects.toMatchObject({
        code,
        status,
      });
      expect(nativeRecordDetailsFailure(new Error(code))).toBeUndefined();
    }
    expect(
      nativeRecordDetailsFailure(
        new NativeRecordDetailsError('NATIVE_RECORD_DETAILS_CONTEXT_CHANGED'),
      ),
    ).toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(
      nativeRecordDetailsFailure(
        new NativeRecordDetailsError('NATIVE_RECORD_DETAILS_CONFIGURATION_INVALID'),
      ),
    ).toMatchObject({ status: 503, code: 'NATIVE_RECORD_DETAILS_UNAVAILABLE' });
    expect(
      nativeRecordDetailsFailure(
        new NativeRecordDetailsError('NATIVE_RECORD_DETAILS_REQUEST_INVALID'),
      ),
    ).toMatchObject({ status: 422, code: 'RECORD_TARGET_REQUIRED' });
  });
});
