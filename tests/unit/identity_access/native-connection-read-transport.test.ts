import { beforeEach, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

const fetchMock = vi.fn();
vi.stubGlobal('fetch', fetchMock);
const deps = vi.hoisted(() => ({ env: vi.fn(), resources: vi.fn(), plan: vi.fn() }));
vi.mock('@erpx/server-runtime/public/runtime', () => ({
  runtimeEnv: deps.env,
  holdingResources: deps.resources,
}));
vi.mock('../../../packages/server-identity_access/src/adapters/native-control-read-plan', () => ({
  planIdentityControlRead: deps.plan,
  PC_CONNECTION_ROUTE: 'pc_connection_route',
}));

import {
  nativeConnectionById,
  nativeConnectionRead,
  routedConnection,
} from '@erpx/server-identity_access/adapters/directory-lookup';

const id = '11111111-2222-4333-8444-555555555555';
const companyId = 'acme-company';
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44998',
  timeoutMs: 2000,
  holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const row = {
  id,
  company_id: companyId,
  name: 'Acme AD',
  provider: 'active_directory',
  config: { mode: 'ldaps', secretProfile: 'ad', authority: 'ad.acme.test' },
  status: 'active',
  version: 2,
  tested_version: 1,
  last_sync_at: 1_700_000_000_000,
  health: 'healthy',
  last_error: null,
  created_by: 'actor-1',
  created_at: 1_699_000_000_000,
};
const reply = (body: unknown, status = 200) => ({
  status,
  body: { cancel: () => Promise.resolve() },
  text: async () => JSON.stringify(body),
}) as unknown as Response;

const first = vi.fn(async () => ({ holding_id: 'h001', company_id: companyId }));
const bind = vi.fn(() => ({ first }));
const prepare = vi.fn(() => ({ bind }));
const write = { prepare: vi.fn() };

beforeEach(() => {
  fetchMock.mockReset();
  deps.env.mockReset();
  deps.resources.mockReset();
  deps.plan.mockReset();
  write.prepare.mockReset();
  first.mockClear();
  deps.env.mockResolvedValue({ CONTROL_DB: { prepare } });
  deps.resources.mockResolvedValue({ write });
  deps.plan.mockResolvedValue({ sql: 'SELECT connection route', values: [id] });
  process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG = config;
});

it('reads the complete retired row from the mounted native connection-read route', async () => {
  fetchMock.mockResolvedValueOnce(reply({ data: row }));
  const actual = await nativeConnectionRead('h001', companyId, id);
  expect(actual).toEqual(row);
  expect(Object.keys(actual).length).toBe(13);
  expect(fetchMock).toHaveBeenCalledTimes(1);
  const [url, init] = fetchMock.mock.calls[0] as [URL, RequestInit];
  expect(String(url)).toBe('http://127.0.0.1:44998/api/v1/holdings/acme/identity-access/connection-read');
  expect(init.method).toBe('POST');
  expect(JSON.parse(String(init.body))).toEqual({ company_id: companyId, id });
  const headers = new Headers(init.headers);
  expect(headers.get('host')).toBe('acme.jesadakorn.com');
  expect(headers.get('cache-control')).toBe('no-cache');
});

it('fails closed without a config, without a registered target, or on a failed read', async () => {
  delete process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
  await expect(nativeConnectionRead('h001', companyId, id)).rejects.toMatchObject({ status: 503 });
  expect(fetchMock).not.toHaveBeenCalled();

  process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG = config;
  await expect(nativeConnectionRead('h999', companyId, id)).rejects.toMatchObject({
    code: 'NATIVE_CONNECTION_READ_TARGET_INVALID', status: 503,
  });
  expect(fetchMock).not.toHaveBeenCalled();

  fetchMock.mockResolvedValueOnce(reply({ error: { code: 'CONNECTION_NOT_FOUND', message: 'Missing.' } }, 404));
  await expect(nativeConnectionRead('h001', companyId, id)).rejects.toMatchObject({ status: 503 });
});

it('rejects a native reply that is not the exact requested connection row', async () => {
  fetchMock.mockResolvedValueOnce(reply({ data: { ...row, id: 'other-connection' } }));
  await expect(nativeConnectionRead('h001', companyId, id)).rejects.toMatchObject({
    code: 'NATIVE_CONNECTION_READ_RESPONSE_INVALID', status: 502,
  });
  fetchMock.mockResolvedValueOnce(reply({ data: { ...row, version: '2' } }));
  await expect(nativeConnectionRead('h001', companyId, id)).rejects.toMatchObject({
    code: 'NATIVE_CONNECTION_READ_RESPONSE_INVALID', status: 502,
  });
});

it('routes the login read through the native transport without preparing local connection SQL', async () => {
  fetchMock.mockResolvedValueOnce(reply({ data: row }));
  expect(await nativeConnectionById(id)).toEqual(row);
  expect(deps.plan).toHaveBeenCalledWith('pc_connection_route', [id]);
  expect(prepare).toHaveBeenCalledTimes(1);
  expect(bind).toHaveBeenCalledTimes(1);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it('routedConnection keeps the retained DB shape but takes the connection from the native lane', async () => {
  fetchMock.mockResolvedValueOnce(reply({ data: row }));
  const routed = await routedConnection(id);
  expect(routed.connection).toEqual(row);
  expect(routed.holdingId).toBe('h001');
  expect(routed.db).toBe(write);
  expect(write.prepare).not.toHaveBeenCalled();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});
