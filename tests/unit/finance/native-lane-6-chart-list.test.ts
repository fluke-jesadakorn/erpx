import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';

vi.mock('server-only', () => ({ }));

import { NATIVE_FINANCE_LANE_2_CONFIG_KEY } from '../../../packages/server-finance/src/adapters/native-lane-2/client';
import { financeChartListNative } from '../../../packages/server-finance/src/adapters/native-lane-6/client';

const actor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme:co-1' };
const original = process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
const chart = {
  id: 'chart-1',
  company_id: 'acme:co-1',
  name: 'Operations ledger',
  base_currency: 'THB',
  accounting_basis: 'accrual',
  status: 'active',
  version: 1,
  created_by: 'user-1',
  created_at: '2026-01-01T00:00:00.000Z',
  updated_at: '2026-01-01T00:00:00.000Z',
};

const setConfig = () => {
  process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44997',
    timeoutMs: 1000,
    holdings: [
      {
        origin: 'https://acme.jesadakorn.com',
        host: 'acme.jesadakorn.com',
        holdingId: 'h001',
        holdingSlug: 'acme',
        companyId: 'acme:co-1',
      },
      {
        origin: 'https://northstar.jesadakorn.com',
        host: 'northstar.jesadakorn.com',
        holdingId: 'h002',
        holdingSlug: 'northstar',
        companyId: 'northstar:co-1',
      },
    ],
  });
};

const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });

describe('native Finance lane 6 chart-list transport', () => {
  beforeEach(() => setConfig());
  afterEach(() => {
    if (original === undefined) delete process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
    else process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY] = original;
    vi.unstubAllGlobals();
  });

  it('returns every chart column from the owner and binds the exact Holding context', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ data: [chart] }));
    expect(await financeChartListNative(actor, fetchMock)).toEqual([chart]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44997/api/v1/holdings/acme/finance/native-lane-6/chart-list',
    );
    const headers = new Headers(init.headers as HeadersInit);
    expect(headers.get('host')).toBe('acme.jesadakorn.com');
    expect(headers.get('origin')).toBe('https://acme.jesadakorn.com');
    expect(headers.get('x-erpx-context-company')).toBe('acme:co-1');
  });

  it('fails closed on an unknown Holding, a company mismatch and a missing config', async () => {
    const fetchMock = vi.fn();
    await expect(
      financeChartListNative({ ...actor, holdingSlug: 'other' }, fetchMock),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    await expect(
      financeChartListNative({ ...actor, companyId: 'other-co' }, fetchMock),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    delete process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
    await expect(financeChartListNative(actor, fetchMock)).rejects.toMatchObject({
      status: 503,
      code: 'FINANCE_NATIVE_LANE_6_UNAVAILABLE',
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('fails closed on a partial row rather than emitting a truncated payload', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ data: [{ name: 'no id' }] }));
    await expect(financeChartListNative(actor, fetchMock)).rejects.toMatchObject({
      status: 502,
      code: 'FINANCE_NATIVE_LANE_6_UNAVAILABLE',
    });
  });
});

describe('native Finance lane 6 SQL execution and route authority', () => {
  const domain = readFileSync(
    new URL('../../../packages/server-finance/src/adapters/native_lane_6/domain.rs', import.meta.url),
    'utf8',
  );
  const postgres = readFileSync(
    new URL('../../../packages/server-finance/src/adapters/native_lane_6/postgres.rs', import.meta.url),
    'utf8',
  );
  const route = readFileSync(
    new URL(
      '../../../packages/feature-finance/src/routes/api/v1/holdings/[holdingSlug]/finance/route.ts',
      import.meta.url,
    ),
    'utf8',
  );

  it('pins the retired statement head and the owner-executed projection', () => {
    expect(domain).toContain(
      '"SELECT * FROM finance_charts WHERE company_id=? ORDER BY name"',
    );
    expect(domain).toContain(
      '"SELECT to_jsonb(c)::text FROM finance_charts c WHERE c.company_id=$1 ORDER BY c.name"',
    );
    expect(postgres).toContain('sqlx::query_scalar::<_, String>(statement)');
    expect(postgres).toContain('.bind(company_id.as_str())');
    expect(postgres).toContain('sqlx::AssertSqlSafe(PG_CHART_LIST_SQL)');
  });

  it('the feature-finance route no longer delegates charts to the TS authority', () => {
    expect(route).not.toContain('listCharts');
    expect(route).toContain('financeChartListNative(actor)');
    expect(route).toContain('nativeJournalList(actor');
  });
});
