import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';

vi.mock('server-only', () => ({ }));

import { NATIVE_FINANCE_LANE_2_CONFIG_KEY } from '../../../modules/finance/server/src/adapters/native-lane-2/client';
import { financeInvoiceListNative } from '../../../modules/finance/server/src/adapters/native-lane-7/client';

const actor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme:co-1' };
const original = process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
const invoice = {
  id: 'inv-1',
  invoice_number: 'INV-1',
  kind: 'purchase',
  invoice_date: '2026-01-01',
  due_date: '2026-01-31',
  currency: 'THB',
  total_minor: 1000,
  outstanding_minor: 0,
  status: 'draft',
  party_name: 'Acme',
};

const setConfig = () => {
  process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44998',
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

describe('native Finance lane 7 invoice-list transport', () => {
  beforeEach(() => setConfig());
  afterEach(() => {
    if (original === undefined) delete process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
    else process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY] = original;
    vi.unstubAllGlobals();
  });

  it('returns every retired invoice column from the owner and binds the exact Holding context', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ data: [invoice] }));
    expect(await financeInvoiceListNative(actor, 100, fetchMock)).toEqual([invoice]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe(
      'http://127.0.0.1:44998/api/v1/holdings/acme/finance/native-lane-7/invoice-list?limit=100',
    );
    const headers = new Headers(init.headers as HeadersInit);
    expect(headers.get('host')).toBe('acme.jesadakorn.com');
    expect(headers.get('origin')).toBe('https://acme.jesadakorn.com');
    expect(headers.get('x-erpx-context-company')).toBe('acme:co-1');
  });

  it('clamps the caller limit to the same owner bound as the retired read', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ data: [invoice] }));
    await financeInvoiceListNative(actor, 5000, fetchMock);
    const [url] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.searchParams.get('limit')).toBe('100');
    await financeInvoiceListNative(actor, 0, fetchMock);
    const [zeroUrl] = fetchMock.mock.calls[1] as unknown as [URL, RequestInit];
    expect(zeroUrl.searchParams.get('limit')).toBe('1');
  });

  it('fails closed on an unknown Holding, a company mismatch and a missing config', async () => {
    const fetchMock = vi.fn();
    await expect(
      financeInvoiceListNative({ ...actor, holdingSlug: 'other' }, 100, fetchMock),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    await expect(
      financeInvoiceListNative({ ...actor, companyId: 'other-co' }, 100, fetchMock),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    delete process.env[NATIVE_FINANCE_LANE_2_CONFIG_KEY];
    await expect(financeInvoiceListNative(actor, 100, fetchMock)).rejects.toMatchObject({
      status: 503,
      code: 'FINANCE_NATIVE_LANE_7_UNAVAILABLE',
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('fails closed on a partial row rather than emitting a truncated payload', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ data: [{ id: 'inv-1' }] }));
    await expect(financeInvoiceListNative(actor, 100, fetchMock)).rejects.toMatchObject({
      status: 502,
      code: 'FINANCE_NATIVE_LANE_7_UNAVAILABLE',
    });
  });
});

describe('native Finance lane 7 SQL execution and route authority', () => {
  const domain = readFileSync(
    new URL('../../../modules/finance/server/src/adapters/native_lane_7/domain.rs', import.meta.url),
    'utf8',
  );
  const postgres = readFileSync(
    new URL('../../../modules/finance/server/src/adapters/native_lane_7/postgres.rs', import.meta.url),
    'utf8',
  );
  const adapter = readFileSync(
    new URL('../../../modules/finance/server/src/adapters/finance-invoice-read.ts', import.meta.url),
    'utf8',
  );
  const route = readFileSync(
    new URL(
      '../../../modules/finance/ui/src/routes/api/v1/holdings/[holdingSlug]/finance/route.ts',
      import.meta.url,
    ),
    'utf8',
  );

  it('pins the retired statement head and the owner-executed projection', () => {
    expect(domain).toContain(
      '"SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name\\n    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id\\n    WHERE i.company_id=? ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT ?"',
    );
    expect(domain).toContain('to_jsonb(listed)::text');
    expect(domain).toContain('LIMIT $2) listed');
    expect(postgres).toContain('sqlx::query_scalar::<_, String>(statement)');
    expect(postgres).toContain('.bind(company_id.as_str())');
    expect(postgres).toContain('.bind(limit)');
    expect(postgres).toContain('sqlx::AssertSqlSafe(PG_INVOICE_LIST_SQL)');
  });

  it('the TS authority is deleted and the route calls the owner directly', () => {
    expect(adapter).not.toContain('FINANCE_RD_INVOICE2_INVOICES');
    expect(adapter).toContain('financeInvoiceListNative(facts.actor, facts.limit)');
    expect(route).toContain('financeInvoiceListNative(actor)');
    expect(route).not.toContain('listInvoices(actor)');
  });
});
