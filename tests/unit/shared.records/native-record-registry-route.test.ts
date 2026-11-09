import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const state = vi.hoisted(() => ({
  actor: {
    userId: 'actor',
    holdingId: 'h001',
    companyId: 'company',
    branchId: null,
    departmentId: null,
    assignmentId: 'assignment-actor',
    role: 'operations_manager',
    assignmentScope: 'holding',
    scope: { kind: 'holding', holdingSlug: 'acme' },
    modules: ['operations'],
    authority: { revision: 9, assignmentVersion: 3 },
    iam: { permissions: ['records.read'] },
  },
  available: true,
  page: vi.fn(),
  list: vi.fn(),
  create: vi.fn(),
  update: vi.fn(),
  archive: vi.fn(),
  reference: vi.fn(),
  context: vi.fn(),
  transition: vi.fn(),
  retainedList: vi.fn(),
  retainedPage: vi.fn(),
  retainedGet: vi.fn(),
  retainedUpdate: vi.fn(),
  retainedArchive: vi.fn(),
  retainedReference: vi.fn(),
  retainedTransition: vi.fn(),
}));

vi.mock('@erpx/server-identity_access/public/authorization', () => ({
  requireWorkContext: async () => state.actor,
}));
vi.mock('@erpx/server-manage_erpx/public/public-origin', () => ({
  requestPublicOrigin: () => 'https://acme.example.com',
}));
vi.mock('@erpx/server-records/adapters/records', () => ({
  listRecords: state.retainedList,
  recordsPage: state.retainedPage,
  createRecord: vi.fn(),
  createRecordSchema: { safeParse: (value: unknown) => ({ success: true, data: value }) },
  getRecord: state.retainedGet,
  updateRecord: state.retainedUpdate,
  updateRecordSchema: { safeParse: (value: unknown) => ({ success: true, data: value }) },
  archiveRecord: state.retainedArchive,
  addReference: state.retainedReference,
  addReferenceSchema: { safeParse: (value: unknown) => ({ success: true, data: value }) },
  transitionRecord: state.retainedTransition,
  transitionSchema: { safeParse: (value: unknown) => ({ success: true, data: value }) },
  recordContext: vi.fn(),
}));
vi.mock('@erpx/server-records/public/mention-display', () => ({
  recordMentionDisplay: vi.fn(),
}));
vi.mock('@erpx/server-records/public/native-record-context', () => ({
  createNativeRecordContextClient: () => ({ context: state.context }),
  nativeRecordContextAvailable: () => false,
  nativeRecordContextConfiguration: () => ({}),
  nativeRecordContextFailure: () => undefined,
}));
vi.mock('@erpx/server-records/public/native-record-registry', () => ({
  nativeRecordRegistryAvailable: () => state.available,
  nativeRecordRegistryConfiguration: () => ({}),
  createNativeRecordRegistryClient: () => ({
    page: state.page,
    list: state.list,
    create: state.create,
    update: state.update,
    archive: state.archive,
    reference: state.reference,
    transition: state.transition,
  }),
  nativeRecordRegistryFailure: (error: unknown) =>
    error instanceof Error
      ? {
          status: 503,
          code: 'NATIVE_RECORD_REGISTRY_UNAVAILABLE',
          message: 'The native Records registry reader is unavailable.',
        }
      : undefined,
}));

import { GET, POST } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/route';
import { PATCH, DELETE } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/route';
import { POST as POST_REFERENCE } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/references/route';
import { POST as POST_TRANSITION } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/transition/route';

const params = { params: Promise.resolve({ holdingSlug: 'acme', module: 'operations' }) };
const request = (query: string) =>
  new Request(`https://acme.example.com/api/v1/holdings/acme/records/operations?${query}`);
const page = {
  items: [],
  nextCursor: null,
  previousCursor: null,
  hasMore: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  state.available = true;
  process.env.ERPX_NATIVE_RECORDS_CONFIG = '{}';
  state.retainedList.mockResolvedValue([]);
  state.retainedPage.mockResolvedValue(page);
  state.retainedGet.mockResolvedValue({ id: 'retained' });
});
afterEach(() => {
  delete process.env.ERPX_NATIVE_RECORDS_CONFIG;
});

it('uses only the native paged registry when configuration is present', async () => {
  state.page.mockResolvedValue(page);
  const response = await GET(request('section=overview&paged=1&q=needle'), params);
  expect(response.status).toBe(200);
  expect(await response.json()).toMatchObject({ data: page });
  expect(state.page).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({ holdingId: 'h001', companyId: 'company' }),
    { holdingSlug: 'acme', moduleId: 'operations', sectionId: 'overview', q: 'needle', cursor: undefined },
  );
  expect(state.retainedPage).not.toHaveBeenCalled();
  expect(state.retainedList).not.toHaveBeenCalled();
});

it('uses only the native unpaged registry when configuration is present', async () => {
  state.list.mockResolvedValue([]);
  const response = await GET(request('section=overview'), params);
  expect(response.status).toBe(200);
  expect(await response.json()).toMatchObject({ data: [] });
  expect(state.list).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({ holdingId: 'h001' }),
    { holdingSlug: 'acme', moduleId: 'operations', sectionId: 'overview' },
  );
  expect(state.retainedList).not.toHaveBeenCalled();
});

it('fails closed on a configured native failure and never reads the retained owner', async () => {
  state.page.mockRejectedValue(new Error('native offline'));
  state.list.mockRejectedValue(new Error('native offline'));
  for (const query of ['section=overview&paged=1', 'section=overview']) {
    const response = await GET(request(query), params);
    expect(response.status).toBe(503);
    expect((await response.json()).error.code).toBe('NATIVE_RECORD_REGISTRY_UNAVAILABLE');
  }
  expect(state.retainedPage).not.toHaveBeenCalled();
  expect(state.retainedList).not.toHaveBeenCalled();
});

it('retains the existing owner path only when native configuration is absent', async () => {
  state.available = false;
  const paged = await GET(request('section=overview&paged=1'), params);
  expect(paged.status).toBe(200);
  expect(state.retainedPage).toHaveBeenCalledWith(state.actor, 'operations', 'overview', {
    q: undefined,
    cursor: undefined,
  });
  const listed = await GET(request('section=overview'), params);
  expect(listed.status).toBe(200);
  expect(state.retainedList).toHaveBeenCalledWith(state.actor, 'operations', 'overview');
  expect(state.page).not.toHaveBeenCalled();
  expect(state.list).not.toHaveBeenCalled();
});

it('uses native create with the current authority then reads back the created record', async () => {
  const record = {
    moduleId: 'support',
    id: 'rec-support-records:http-1',
    sectionId: 'tickets',
    title: 'Native record',
    description: '',
    status: 'draft',
    fields: [{ key: 'priority', label: 'Priority', type: 'text', value: 'high' }],
    version: 1,
    createdBy: 'actor',
    updatedAt: 1,
  };
  state.create.mockResolvedValue(record);
  const input = {
    sectionId: 'tickets',
    title: 'Native record',
    description: '',
    fields: [{ key: 'priority', label: 'Priority', type: 'text', value: 'high' }],
  };
  const request = new Request('https://acme.example.com/api/v1/holdings/acme/records/support', {
    method: 'POST',
    headers: {
      origin: 'https://acme.example.com',
      'idempotency-key': 'records:http-1',
      'content-type': 'application/json',
    },
    body: JSON.stringify(input),
  });
  const response = await POST(request, {
    params: Promise.resolve({ holdingSlug: 'acme', module: 'support' }),
  });
  expect(response.status).toBe(201);
  expect(state.create).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({ userId: 'actor', holdingId: 'h001', holdingSlug: 'acme', branchSlug: null, departmentSlug: null }),
    { holdingSlug: 'acme', moduleId: 'support', ...input },
    'records:http-1',
  );
  expect(state.list).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(record);
});

it('uses native update for support with the current authority and never reads the retained writer', async () => {
  const record = {
    moduleId: 'support',
    id: 'rec-support-records:http-1',
    sectionId: 'tickets',
    title: 'Native record',
    description: '',
    status: 'draft',
    fields: [{ key: 'priority', label: 'Priority', type: 'text', value: 'low' }],
    version: 2,
    createdBy: 'actor',
    updatedAt: 2,
  };
  state.update.mockResolvedValue(record);
  const request = (key: string | null = 'records:http-update-1') =>
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/support/rec-support-records:http-1',
      {
        method: 'PATCH',
        headers: {
          origin: 'https://acme.example.com',
          'content-type': 'application/json',
          ...(key ? { 'idempotency-key': key } : {}),
        },
        body: JSON.stringify({ expectedVersion: 1, fields: { priority: 'low' } }),
      },
    );
  const response = await PATCH(request(), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:http-1',
    }),
  });
  expect(response.status).toBe(200);
  expect(state.update).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({
      userId: 'actor',
      holdingId: 'h001',
      companyId: 'company',
      branchSlug: null,
      departmentSlug: null,
    }),
    {
      holdingSlug: 'acme',
      moduleId: 'support',
      recordId: 'rec-support-records:http-1',
      expectedVersion: 1,
      fields: { priority: 'low' },
    },
    'records:http-update-1',
  );
  expect(state.retainedGet).not.toHaveBeenCalled();
  expect(state.retainedUpdate).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(record);
  const missing = await PATCH(request(null), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:http-1',
    }),
  });
  expect(missing.status).toBe(422);
  expect((await missing.json()).error.code).toBe('IDEMPOTENCY_KEY_REQUIRED');
  expect(state.update).toHaveBeenCalledTimes(1);
});

it('keeps the retained update path outside the bounded support module', async () => {
  const retainedRecord = { id: 'retained', version: 2 };
  state.retainedUpdate.mockResolvedValue(retainedRecord);
  const response = await PATCH(
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/operations/rec-1',
      {
        method: 'PATCH',
        headers: {
          origin: 'https://acme.example.com',
          'idempotency-key': 'records:http-retained',
          'content-type': 'application/json',
        },
        body: JSON.stringify({ expectedVersion: 1, fields: { total: 3 } }),
      },
    ),
    {
      params: Promise.resolve({
        holdingSlug: 'acme',
        module: 'operations',
        id: 'rec-1',
      }),
    },
  );
  expect(response.status).toBe(200);
  expect(state.retainedGet).toHaveBeenCalledWith(state.actor, 'operations', 'rec-1');
  expect(state.retainedUpdate).toHaveBeenCalledWith(
    state.actor,
    'operations',
    'rec-1',
    { expectedVersion: 1, fields: { total: 3 } },
    'records:http-retained',
  );
  expect(state.update).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(retainedRecord);
});

it('uses native archive for support with the current authority and never reads the retained writer', async () => {
  const archived = {
    id: 'rec-support-records:http-1',
    moduleId: 'support',
    archived: true,
  };
  state.archive.mockResolvedValue(archived);
  const request = (key: string | null = 'records:http-archive-1') =>
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/support/rec-support-records:http-1?expectedVersion=1',
      {
        method: 'DELETE',
        headers: {
          origin: 'https://acme.example.com',
          ...(key ? { 'idempotency-key': key } : {}),
        },
      },
    );
  const response = await DELETE(request(), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:http-1',
    }),
  });
  expect(response.status).toBe(200);
  expect(state.archive).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({
      userId: 'actor',
      holdingId: 'h001',
      companyId: 'company',
      branchSlug: null,
      departmentSlug: null,
    }),
    {
      holdingSlug: 'acme',
      moduleId: 'support',
      recordId: 'rec-support-records:http-1',
      expectedVersion: 1,
    },
    'records:http-archive-1',
  );
  expect(state.retainedGet).not.toHaveBeenCalled();
  expect(state.retainedArchive).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(archived);
  const missing = await DELETE(request(null), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:http-1',
    }),
  });
  expect(missing.status).toBe(422);
  expect((await missing.json()).error.code).toBe('IDEMPOTENCY_KEY_REQUIRED');
  expect(state.archive).toHaveBeenCalledTimes(1);
});

it('keeps the retained archive path outside the bounded support module', async () => {
  const retained = { id: 'rec-1', moduleId: 'operations', archived: true };
  state.retainedArchive.mockResolvedValue(retained);
  const response = await DELETE(
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/operations/rec-1?expectedVersion=2',
      {
        method: 'DELETE',
        headers: {
          origin: 'https://acme.example.com',
          'idempotency-key': 'records:http-retained-archive',
        },
      },
    ),
    {
      params: Promise.resolve({
        holdingSlug: 'acme',
        module: 'operations',
        id: 'rec-1',
      }),
    },
  );
  expect(response.status).toBe(200);
  expect(state.retainedArchive).toHaveBeenCalledWith(
    state.actor,
    'operations',
    'rec-1',
    2,
    'records:http-retained-archive',
  );
  expect(state.archive).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(retained);
});

it('uses native reference for support then reads the updated native context', async () => {
  const recordContext = {
    record: { id: 'rec-support-records:host', version: 1 },
    references: [],
    backlinks: [],
    mentions: [],
    suggestions: [],
    history: [],
  };
  state.reference.mockResolvedValue({
    id: 'rec-support-records:host',
    moduleId: 'support',
  });
  state.context.mockResolvedValue(recordContext);
  const request = (key: string | null = 'records:http-reference-1') =>
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/support/rec-support-records:host/references',
      {
        method: 'POST',
        headers: {
          origin: 'https://acme.example.com',
          'content-type': 'application/json',
          ...(key ? { 'idempotency-key': key } : {}),
        },
        body: JSON.stringify({
          targetModule: 'support',
          targetId: 'rec-support-records:target',
          kind: 'dependsOn',
        }),
      },
    );
  const response = await POST_REFERENCE(request(), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:host',
    }),
  });
  expect(response.status).toBe(201);
  expect(state.reference).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({
      userId: 'actor',
      holdingId: 'h001',
      companyId: 'company',
      branchSlug: null,
      departmentSlug: null,
    }),
    {
      holdingSlug: 'acme',
      moduleId: 'support',
      recordId: 'rec-support-records:host',
      targetModule: 'support',
      targetId: 'rec-support-records:target',
      kind: 'dependsOn',
    },
    'records:http-reference-1',
  );
  expect(state.context).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({ holdingId: 'h001', companyId: 'company' }),
    {
      holdingSlug: 'acme',
      moduleId: 'support',
      recordId: 'rec-support-records:host',
    },
  );
  expect(state.retainedReference).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(recordContext);
  const missing = await POST_REFERENCE(request(null), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:host',
    }),
  });
  expect(missing.status).toBe(422);
  expect((await missing.json()).error.code).toBe('IDEMPOTENCY_KEY_REQUIRED');
  expect(state.reference).toHaveBeenCalledTimes(1);
});

it('keeps the retained reference path outside the bounded support module', async () => {
  const retained = { record: { id: 'rec-1' }, references: [] };
  state.retainedReference.mockResolvedValue(retained);
  const response = await POST_REFERENCE(
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/operations/rec-1/references',
      {
        method: 'POST',
        headers: {
          origin: 'https://acme.example.com',
          'idempotency-key': 'records:http-retained-reference',
          'content-type': 'application/json',
        },
        body: JSON.stringify({
          targetModule: 'operations',
          targetId: 'rec-2',
          kind: 'blocks',
        }),
      },
    ),
    {
      params: Promise.resolve({
        holdingSlug: 'acme',
        module: 'operations',
        id: 'rec-1',
      }),
    },
  );
  expect(response.status).toBe(201);
  expect(state.retainedReference).toHaveBeenCalledWith(
    state.actor,
    'operations',
    'rec-1',
    { targetModule: 'operations', targetId: 'rec-2', kind: 'blocks' },
    'records:http-retained-reference',
  );
  expect(state.reference).not.toHaveBeenCalled();
  expect(state.context).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(retained);
});

it('uses native transition for support with the current authority and never reads the retained writer', async () => {
  const transitioned = {
    moduleId: 'support',
    id: 'rec-support-records:host',
    sectionId: 'tickets',
    title: 'Native record',
    description: '',
    status: 'in_review',
    fields: [{ key: 'priority', label: 'Priority', type: 'text', value: 'high' }],
    version: 2,
    createdBy: 'actor',
    updatedAt: 2,
  };
  state.transition.mockResolvedValue(transitioned);
  const request = (key: string | null = 'records:http-transition-1') =>
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/support/rec-support-records:host/transition',
      {
        method: 'POST',
        headers: {
          origin: 'https://acme.example.com',
          'content-type': 'application/json',
          ...(key ? { 'idempotency-key': key } : {}),
        },
        body: JSON.stringify({ expectedVersion: 1, action: 'submit', note: 'Ready' }),
      },
    );
  const response = await POST_TRANSITION(request(), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:host',
    }),
  });
  expect(response.status).toBe(200);
  expect(state.transition).toHaveBeenCalledWith(
    expect.any(Headers),
    expect.objectContaining({
      userId: 'actor',
      holdingId: 'h001',
      companyId: 'company',
      branchSlug: null,
      departmentSlug: null,
    }),
    {
      holdingSlug: 'acme',
      moduleId: 'support',
      recordId: 'rec-support-records:host',
      expectedVersion: 1,
      action: 'submit',
      note: 'Ready',
    },
    'records:http-transition-1',
  );
  expect(state.retainedTransition).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(transitioned);
  const missing = await POST_TRANSITION(request(null), {
    params: Promise.resolve({
      holdingSlug: 'acme',
      module: 'support',
      id: 'rec-support-records:host',
    }),
  });
  expect(missing.status).toBe(422);
  expect((await missing.json()).error.code).toBe('COMMAND_KEY_REQUIRED');
  expect(state.transition).toHaveBeenCalledTimes(1);
});

it('keeps the retained transition path outside the bounded support module', async () => {
  const retained = {
    moduleId: 'operations',
    id: 'rec-1',
    status: 'in_review',
    version: 2,
  };
  state.retainedTransition.mockResolvedValue(retained);
  const response = await POST_TRANSITION(
    new Request(
      'https://acme.example.com/api/v1/holdings/acme/records/operations/rec-1/transition',
      {
        method: 'POST',
        headers: {
          origin: 'https://acme.example.com',
          'idempotency-key': 'records:http-retained-transition',
          'content-type': 'application/json',
        },
        body: JSON.stringify({ expectedVersion: 1, action: 'submit', note: '' }),
      },
    ),
    {
      params: Promise.resolve({
        holdingSlug: 'acme',
        module: 'operations',
        id: 'rec-1',
      }),
    },
  );
  expect(response.status).toBe(200);
  expect(state.retainedTransition).toHaveBeenCalledWith(
    state.actor,
    'operations',
    'rec-1',
    { expectedVersion: 1, action: 'submit', note: '' },
    'records:http-retained-transition',
  );
  expect(state.transition).not.toHaveBeenCalled();
  expect((await response.json()).data).toEqual(retained);
});
