import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
const incomingHeaders = vi.hoisted(() => ({ value: new Headers() }));
vi.mock('next/headers', () => ({ headers: async () => incomingHeaders.value }));

import {
  nativeAttachmentDescriptor,
  nativeMaintenanceLane36Configured,
  requireAttachmentDescriptor,
} from '../../../modules/maintenance/server/src/adapters/native-lane-36/client';

const ENV = 'ERPX_NATIVE_MAINTENANCE_LANE36_CONFIG';
/** Exact development configuration pin: three registered Holdings. */
const DEV_CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    {
      origin: 'https://acme.jesadakorn.com',
      host: 'acme.jesadakorn.com',
      holdingId: 'h001',
      holdingSlug: 'acme',
      companyId: 'company-acme',
    },
    {
      origin: 'https://northstar.jesadakorn.com',
      host: 'northstar.jesadakorn.com',
      holdingId: 'h002',
      holdingSlug: 'northstar',
      companyId: 'company-northstar',
    },
    {
      origin: 'https://erpx-internal.jesadakorn.com',
      host: 'erpx-internal.jesadakorn.com',
      holdingId: 'h003',
      holdingSlug: 'erpx',
      companyId: 'company-erpx',
    },
  ],
} as const;
const config = (mutate: (value: Record<string, any>) => void = () => undefined) => {
  const value = JSON.parse(JSON.stringify(DEV_CONFIG));
  mutate(value);
  return JSON.stringify(value);
};
const acme = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-acme' };
const northstar = { holdingId: 'h002', holdingSlug: 'northstar', companyId: 'company-northstar' };
const erpx = { holdingId: 'h003', holdingSlug: 'erpx', companyId: 'company-erpx' };

const descriptor = (companyId: string) => ({
  company_id: companyId, branch_id: 'branch-1', department_id: null, version: 3,
});
const ok = (value: unknown, targetId = 'work-1', companyId = 'company-acme') =>
  new Response(JSON.stringify({ data: value, targetId, companyId }), {
    status: 200, headers: { 'content-type': 'application/json' },
  });
const incoming = (host: string) => new Headers({ host, cookie: 'session=abc' });

beforeEach(() => {
  process.env[ENV] = config();
  incomingHeaders.value = incoming('acme.jesadakorn.com');
});
afterEach(() => {
  delete process.env[ENV];
  vi.unstubAllGlobals();
});

describe('native Maintenance lane-36 descriptor transport', () => {
  it('routes every registered Holding from its actor to its own host and Company', async () => {
    const transport = vi.fn().mockResolvedValue(ok(descriptor('company-acme')));
    vi.stubGlobal('fetch', transport);
    for (const [actor, host, slug, company] of [
      [acme, 'acme.jesadakorn.com', 'acme', 'company-acme'],
      [northstar, 'northstar.jesadakorn.com', 'northstar', 'company-northstar'],
      [erpx, 'erpx-internal.jesadakorn.com', 'erpx', 'company-erpx'],
    ] as const) {
      incomingHeaders.value = incoming(host);
      transport.mockResolvedValue(ok(descriptor(company), 'work-1', company));
      await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', company, actor))
        .resolves.toEqual(descriptor(company));
      const [url, init] = transport.mock.calls.at(-1) as [URL, RequestInit];
      expect(url.origin).toBe('http://127.0.0.1:3198');
      expect(url.pathname).toBe(`/api/v1/holdings/${slug}/maintenance/lane-36/attachment-target`);
      expect(url.searchParams.get('id')).toBe('work-1');
      expect((init.headers as Headers).get('host')).toBe(host);
      expect((init.headers as Headers).get('x-erpx-context-company')).toBe(company);
      expect((init.headers as Headers).get('cookie')).toBe('session=abc');
      expect(JSON.stringify(init)).not.toContain('SELECT');
    }
  });

  it('fails closed for unknown actors, unknown hosts and any first-Holding default', async () => {
    const transport = vi.fn().mockResolvedValue(ok(descriptor('company-acme')));
    vi.stubGlobal('fetch', transport);
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme', {
      holdingId: 'h099', holdingSlug: 'unknown', companyId: 'company-acme',
    })).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme', acme))
      .resolves.toEqual(descriptor('company-acme'));
    incomingHeaders.value = incoming('unknown.jesadakorn.com');
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme'))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    incomingHeaders.value = new Headers();
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme'))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    expect(transport).toHaveBeenCalledTimes(1);
  });

  it('pins a configured companyId and rejects cross-Holding host/company drift', async () => {
    const transport = vi.fn().mockResolvedValue(ok(descriptor('company-acme')));
    vi.stubGlobal('fetch', transport);
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-northstar', acme))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    incomingHeaders.value = incoming('northstar.jesadakorn.com');
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme', acme))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    expect(transport).not.toHaveBeenCalled();
  });

  it('accepts the bounded missing-row response', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok(null, 'missing')));
    await expect(nativeAttachmentDescriptor('maintenance_work', 'missing', 'company-acme', acme))
      .resolves.toBeNull();
  });

  it('fails closed without config and when the owner is unreachable', async () => {
    delete process.env[ENV];
    expect(nativeMaintenanceLane36Configured()).toBe(false);
    await expect(requireAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme'))
      .rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_LANE36_CONFIG_REQUIRED', status: 503 });
    process.env[ENV] = config();
    expect(nativeMaintenanceLane36Configured()).toBe(true);
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('ECONNREFUSED')));
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme', acme))
      .rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_LANE36_UNAVAILABLE', status: 503 });
  });

  it('rejects foreign targets, malformed rows, owner errors and invalid config', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ ...descriptor('company-acme'), company_id: 'foreign' })));
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme', acme))
      .rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_LANE36_RESPONSE_INVALID', status: 502 });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ ...descriptor('company-acme'), extra: 1 })));
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme', acme))
      .rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_LANE36_RESPONSE_INVALID', status: 502 });
    await expect(nativeAttachmentDescriptor('finance_journal', 'work-1', 'company-acme'))
      .rejects.toMatchObject({ code: 'MAINTENANCE_LANE36_QUERY_INVALID', status: 422 });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: { code: 'MAINTENANCE_LANE36_READ_UNAVAILABLE', message: 'down' } }),
      { status: 503, headers: { 'content-type': 'application/json' } },
    )));
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme', acme))
      .rejects.toMatchObject({ code: 'MAINTENANCE_LANE36_READ_UNAVAILABLE', status: 503, message: 'down' });
    process.env[ENV] = config(value => { value.privateBaseUrl = 'http://127.0.0.1:3100'; });
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme'))
      .rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID', status: 503 });
    process.env[ENV] = config(value => { delete value.holdings[2].host; });
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme'))
      .rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID', status: 503 });
    process.env[ENV] = config(value => { value.holdings[2].host = value.holdings[0].host; });
    await expect(nativeAttachmentDescriptor('maintenance_work', 'work-1', 'company-acme'))
      .rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID', status: 503 });
  });
});
