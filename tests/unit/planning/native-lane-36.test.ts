import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
const incomingHeaders = vi.hoisted(() => ({ value: new Headers() }));
vi.mock('next/headers', () => ({ headers: async () => incomingHeaders.value }));

import {
  nativeCalendar,
  nativeComparison,
  nativePlanningLane36Configured,
  nativeRecommendations,
  nativeResources,
  nativeScenario,
  nativeScopedRecommendation,
} from '../../../modules/planning/server/src/adapters/native-lane-36/client';

const ENV = 'ERPX_NATIVE_PLANNING_LANE36_CONFIG';
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

const recommendation = {
  id: 'r1', scenario_id: 's1', scenario_version: 1, recommended_quantity_milli: 2000,
  required_minutes: 60, feasible_date: '2026-10-04', status: 'proposed', created_by: 'planner',
  reviewed_by: null, version: 1, created_at: 100, updated_at: 100,
  company_id: 'company-acme', branch_id: 'branch-1',
};
const resource = {
  id: 'resource', company_id: 'company-acme', branch_id: 'branch-1', code: 'CELL',
  name: 'Assembly cell', daily_minutes: 480, work_weekdays_json: '[1,2,3,4,5]',
  status: 'active', version: 1, created_by: 'planner', created_at: 10, updated_at: 10,
};
const calendar = {
  resource_id: 'resource', work_date: '2026-10-04', available_minutes: 60,
  reason: 'Verified shutdown', version: 1, updated_by: 'planner', updated_at: 20,
};
const scenario = (companyId: string) => ({
  id: 's1', company_id: companyId, branch_id: 'branch-1', warehouse_id: 'warehouse',
  specification_id: 'spec', specification_version: 2, demand_quantity_milli: 2000,
  due_date: '2026-10-04', capacity_minutes: 480, shortages_json: '[]', status: 'calculated',
  created_by: 'planner', version: 1, created_at: 100, updated_at: 100, context_json: '{}',
});
const scopedRecommendation = (companyId: string) => ({
  ...recommendation, company_id: companyId, branch_id: 'branch-1', warehouse_id: 'warehouse',
  specification_id: 'spec', specification_version: 2, context_json: '{}',
  scenario_created_by: 'planner',
});
const comparison = (companyId: string) => ({
  id: 'other', version: 2, shortages_json: '[]', required_minutes: 30, feasible_date: '2026-10-05',
});
const ok = (kind: string, data: unknown, companyId = 'company-acme', branchId: string | null = 'branch-1') =>
  new Response(JSON.stringify({ data, kind, companyId, branchId }), {
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

describe('native Planning lane-36 scoped read transport', () => {
  it('routes every registered Holding from its actor to its own host and Company', async () => {
    const transport = vi.fn().mockImplementation((url: URL) =>
      Promise.resolve(ok('resources', [{ ...resource, company_id: 'company-acme' }]))
    );
    vi.stubGlobal('fetch', transport);
    for (const [actor, host, slug, company] of [
      [acme, 'acme.jesadakorn.com', 'acme', 'company-acme'],
      [northstar, 'northstar.jesadakorn.com', 'northstar', 'company-northstar'],
      [erpx, 'erpx-internal.jesadakorn.com', 'erpx', 'company-erpx'],
    ] as const) {
      incomingHeaders.value = incoming(host);
      transport.mockImplementation(() => Promise.resolve(ok('resources', [
        { ...resource, company_id: company, branch_id: 'branch-1' },
      ], company)));
      await expect(nativeResources(company, 'branch-1', 'branch-1', actor))
        .resolves.toEqual({ results: [{ ...resource, company_id: company, branch_id: 'branch-1' }] });
      const [url, init] = transport.mock.calls.at(-1) as [URL, RequestInit];
      expect(url.origin).toBe('http://127.0.0.1:3198');
      expect(url.pathname).toBe(`/api/v1/holdings/${slug}/planning/lane-36/resources`);
      expect((init.headers as Headers).get('host')).toBe(host);
      expect((init.headers as Headers).get('x-erpx-context-company')).toBe(company);
      expect((init.headers as Headers).get('cookie')).toBe('session=abc');
      expect(JSON.stringify(init)).not.toContain('SELECT');
    }
  });

  it('fails closed for unknown actors, unknown hosts and any first-Holding default', async () => {
    const transport = vi.fn().mockResolvedValue(ok('resources', [resource]));
    vi.stubGlobal('fetch', transport);
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', {
      holdingId: 'h099', holdingSlug: 'unknown', companyId: 'company-acme',
    })).rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme)).resolves.toBeDefined();
    incomingHeaders.value = incoming('unknown.jesadakorn.com');
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1'))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    incomingHeaders.value = new Headers();
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1'))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    expect(transport).toHaveBeenCalledTimes(1);
  });

  it('pins a configured companyId and rejects cross-Holding host/company drift', async () => {
    const transport = vi.fn().mockResolvedValue(ok('resources', [resource]));
    vi.stubGlobal('fetch', transport);
    await expect(nativeResources('company-northstar', 'branch-1', 'branch-1', acme))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    incomingHeaders.value = incoming('northstar.jesadakorn.com');
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme))
      .rejects.toMatchObject({ code: 'WORK_CONTEXT_CHANGED', status: 409 });
    expect(transport).not.toHaveBeenCalled();
  });

  it('validates each of the three closed row shapes and rejects extra or mistyped fields', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('resources', [resource])));
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme))
      .resolves.toEqual({ results: [resource] });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('calendar', [calendar], 'company-acme')));
    await expect(nativeCalendar('company-acme', 'branch-1', 'branch-1', acme))
      .resolves.toEqual({ results: [calendar] });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('recommendations', [recommendation])));
    await expect(nativeRecommendations('company-acme', 'branch-1', 'branch-1', acme))
      .resolves.toEqual({ results: [recommendation] });
    for (const bad of [
      { ...resource, extra: 1 },
      { ...resource, daily_minutes: '480' },
      { ...calendar, available_minutes: 2000 },
    ]) {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('resources', [bad])));
      await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme))
        .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_RESPONSE_INVALID', status: 502 });
    }
  });

  it('fails closed without config and when the owner is unreachable', async () => {
    delete process.env[ENV];
    expect(nativePlanningLane36Configured()).toBe(false);
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1'))
      .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_CONFIG_REQUIRED', status: 503 });
    process.env[ENV] = config();
    expect(nativePlanningLane36Configured()).toBe(true);
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('ECONNREFUSED')));
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme))
      .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_UNAVAILABLE', status: 503 });
  });

  it('rejects scope drift, unbounded input, malformed envelopes and invalid config', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('calendar', [calendar], 'company-acme', 'other')));
    await expect(nativeCalendar('company-acme', 'branch-1', 'branch-1', acme))
      .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_RESPONSE_INVALID', status: 502 });
    await expect(nativeResources('company-acme', 'x/y', 'x/y', acme))
      .rejects.toMatchObject({ code: 'PLANNING_LANE36_QUERY_INVALID', status: 422 });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('resources', [{ ...resource, company_id: 'foreign' }])));
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme))
      .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_RESPONSE_INVALID', status: 502 });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', {
      status: 502, headers: { 'content-type': 'application/json' },
    })));
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme))
      .rejects.toMatchObject({ status: 503 });
    for (const privateBaseUrl of [
      'http://127.0.0.1:3100', 'https://127.0.0.1:3198', 'http://example.com:3198',
    ]) {
      process.env[ENV] = config(value => { value.privateBaseUrl = privateBaseUrl; });
      await expect(nativeResources('company-acme', 'branch-1', 'branch-1'))
        .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_CONFIG_INVALID', status: 503 });
    }
    process.env[ENV] = config(value => { delete value.holdings[1].host; });
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1'))
      .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_CONFIG_INVALID', status: 503 });
    process.env[ENV] = config(value => { value.holdings[1].host = value.holdings[0].host; });
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1'))
      .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_CONFIG_INVALID', status: 503 });
  });

  it('routes the three row reads from every Holding with closed identity params', async () => {
    const transport = vi.fn().mockResolvedValue(ok('resources', [resource]));
    vi.stubGlobal('fetch', transport);
    for (const [actor, host, slug, company] of [
      [acme, 'acme.jesadakorn.com', 'acme', 'company-acme'],
      [northstar, 'northstar.jesadakorn.com', 'northstar', 'company-northstar'],
      [erpx, 'erpx-internal.jesadakorn.com', 'erpx', 'company-erpx'],
    ] as const) {
      incomingHeaders.value = incoming(host);
      transport.mockResolvedValue(ok('scenario', scenario(company), company));
      await expect(nativeScenario({ id: 's1' }, company, 'branch-1', actor))
        .resolves.toEqual(scenario(company));
      const [url, init] = transport.mock.calls.at(-1) as [URL, RequestInit];
      expect(url.pathname).toBe(`/api/v1/holdings/${slug}/planning/lane-36/scenario`);
      expect(url.searchParams.get('id')).toBe('s1');
      expect(url.searchParams.get('branch')).toBe('branch-1');
      expect((init.headers as Headers).get('host')).toBe(host);
      expect((init.headers as Headers).get('x-erpx-context-company')).toBe(company);
    }
    incomingHeaders.value = incoming('acme.jesadakorn.com');
    transport.mockResolvedValue(ok('scoped-recommendation', scopedRecommendation('company-acme')));
    await expect(nativeScopedRecommendation({ id: 'r1' }, 'company-acme', 'branch-1', acme))
      .resolves.toEqual(scopedRecommendation('company-acme'));
    expect((transport.mock.calls.at(-1) as [URL])[0].pathname)
      .toBe('/api/v1/holdings/acme/planning/lane-36/scoped-recommendation');
    transport.mockResolvedValue(ok('comparison', comparison('company-acme')));
    await expect(nativeComparison(
      { comparisonScenarioId: 'other', specificationId: 'spec' },
      'company-acme', 'branch-1', acme,
    )).resolves.toEqual(comparison('company-acme'));
    const [comparisonUrl] = transport.mock.calls.at(-1) as [URL];
    expect(comparisonUrl.searchParams.get('comparisonScenarioId')).toBe('other');
    expect(comparisonUrl.searchParams.get('specificationId')).toBe('spec');
  });

  it('fails closed on missing identity, malformed rows and accepts a bounded null row', async () => {
    await expect(nativeScenario({}, 'company-acme', 'branch-1', acme))
      .rejects.toMatchObject({ code: 'PLANNING_LANE36_QUERY_INVALID', status: 422 });
    await expect(nativeComparison(
      { comparisonScenarioId: 'other' }, 'company-acme', 'branch-1', acme,
    )).rejects.toMatchObject({ code: 'PLANNING_LANE36_QUERY_INVALID', status: 422 });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('scenario', null)));
    await expect(nativeScenario({ id: 's1' }, 'company-acme', 'branch-1', acme))
      .resolves.toBeNull();
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('scenario', {
      ...scenario('company-acme'), extra: 1,
    })));
    await expect(nativeScenario({ id: 's1' }, 'company-acme', 'branch-1', acme))
      .rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_RESPONSE_INVALID', status: 502 });
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok('comparison', {
      ...comparison('company-acme'), version: '2',
    })));
    await expect(nativeComparison(
      { comparisonScenarioId: 'other', specificationId: 'spec' },
      'company-acme', 'branch-1', acme,
    )).rejects.toMatchObject({ code: 'NATIVE_PLANNING_LANE36_RESPONSE_INVALID', status: 502 });
  });

  it('surfaces a bounded owner error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(
      JSON.stringify({ error: { code: 'PLANNING_LANE36_READ_UNAVAILABLE', message: 'down' } }),
      { status: 503, headers: { 'content-type': 'application/json' } },
    )));
    await expect(nativeResources('company-acme', 'branch-1', 'branch-1', acme)).rejects.toMatchObject({
      code: 'PLANNING_LANE36_READ_UNAVAILABLE', status: 503, message: 'down',
    });
  });
});
