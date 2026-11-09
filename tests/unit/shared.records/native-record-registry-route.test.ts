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
  retainedList: vi.fn(),
  retainedPage: vi.fn(),
}));

vi.mock('@erpx/server-identity_access/public/authorization', () => ({
  requireWorkContext: async () => state.actor,
}));
vi.mock('@erpx/server-records/adapters/records', () => ({
  listRecords: state.retainedList,
  recordsPage: state.retainedPage,
  createRecord: vi.fn(),
  createRecordSchema: { safeParse: (value: unknown) => ({ success: true, data: value }) },
}));
vi.mock('@erpx/server-records/public/native-record-registry', () => ({
  nativeRecordRegistryAvailable: () => state.available,
  nativeRecordRegistryConfiguration: () => ({}),
  createNativeRecordRegistryClient: () => ({ page: state.page, list: state.list, create: state.create }),
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
