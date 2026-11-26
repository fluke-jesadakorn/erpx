import { beforeEach, expect, it, vi } from 'vitest';
import { ApiError } from '@erpx/server-runtime/public/http';

const state = vi.hoisted(() => ({
  read: vi.fn(),
  actor: undefined as unknown,
  authority: undefined as unknown,
  id: undefined as unknown,
}));

vi.mock('../../../packages/server-legal/src/adapters/native-processing-status/client', () => ({
  nativeProcessingStatusRead: async (
    actor: unknown,
    authority: unknown,
    id: unknown,
  ) => {
    state.actor = actor;
    state.authority = authority;
    state.id = id;
    return state.read();
  },
}));
vi.mock('@erpx/server-identity_access/public/authorization', () => ({
  requireWorkContext: async () => ({
    holdingId: 'h001',
    holdingSlug: 'acme',
    companyId: 'acme-co',
    userId: 'viewer',
    modules: ['legal'],
  }),
}));
vi.mock('@erpx/server-runtime/public/runtime', () => ({
  holdingResources: async () => ({ write: {}, files: {} }),
}));
vi.mock('@erpx/server-manage_erpx/public/public-origin', () => ({
  requestPublicOrigin: () => 'https://dev-erpx.example.test',
}));

import { GET } from '@erpx/feature-legal/routes/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route';

const read = {
  id: 'job-1',
  item_id: 'item-1',
  document_id: 'doc-1',
  state: 'queued',
  error: null,
  attempts: 1,
  jobId: 'job-1',
  result: null,
};

function get() {
  return GET(
    new Request('https://dev-erpx.example.test/api/v1/holdings/acme/legal/processing/job-1'),
    { params: Promise.resolve({ holdingSlug: 'acme', path: ['processing', 'job-1'] }) },
  );
}

beforeEach(() => {
  state.read.mockReset().mockResolvedValue(read);
  state.actor = undefined;
  state.authority = undefined;
  state.id = undefined;
});

it('serves the processing-status GET from the native read with a bounded actor and authority', async () => {
  const response = await get();
  expect(response.status).toBe(200);
  expect(await response.json()).toMatchObject({ data: read });
  expect(state.actor).toEqual({ holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co' });
  expect(state.authority).toEqual({ itemAccessible: true });
  expect(state.id).toBe('job-1');
});

it('fails closed with 503 when the native read is unavailable', async () => {
  state.read.mockRejectedValue(new ApiError(503, 'NATIVE_LEGAL_PROCESSING_STATUS_UNAVAILABLE', 'unavailable'));
  const response = await get();
  expect(response.status).toBe(503);
  expect(await response.json()).toMatchObject({ error: { code: 'NATIVE_LEGAL_PROCESSING_STATUS_UNAVAILABLE' } });
});
