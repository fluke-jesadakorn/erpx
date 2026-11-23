import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativeProcurementReadAll,
  nativeProcurementReadFirst,
  PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE,
  PROCUREMENT_RD_REQUEST_QUOTATION,
} from '../../../packages/server-procurement/src/adapters/native-procurement-read';
import { procurementInvoiceMatchForFinance } from '../../../packages/server-procurement/src/adapters/finance-entrypoint-queries';
import { requestQuotation } from '../../../packages/server-procurement/src/adapters/people-app-entrypoint-facts';

const envKey = 'ERPX_NATIVE_PROCUREMENT_CONFIG';
const original = process.env[envKey];
const planned = {
  [PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE]: 'SELECT id,reference FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?',
  [PROCUREMENT_RD_REQUEST_QUOTATION]: "SELECT id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled'",
};
const stubFetch = (payload: unknown, status = 200) => {
  const fetchMock = vi.fn(async () => ({
    status,
    body: { cancel: vi.fn(async () => undefined) },
    json: async () => payload,
  }));
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
};

beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44997',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = (rows: Record<string, unknown>[] = [{ id: 'row-1' }]) => {
  const bound: unknown[][] = [];
  const prepared: string[] = [];
  let batched: { sql: string; values: unknown[] }[] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
      };
    },
    async batch(statements: { __sql?: string; __values?: unknown[] }[]) {
      batched = statements.map((s) => ({ sql: s.__sql ?? prepared[0] ?? '', values: s.__values ?? bound[0] ?? [] }));
      return [{ results: rows, success: true, meta: { changes: 0, duration: 0 } }];
    },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared, get batched() { return batched; } };
};

describe('native Procurement caller-unit read plan', () => {
  it('exposes allow-listed PROCUREMENT_RD_* kinds with no SQL on the wire', () => {
    expect(PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE).toBe('procurement_rd_invoice_match_finance');
    expect(PROCUREMENT_RD_REQUEST_QUOTATION).toBe('procurement_rd_request_quotation');
    for (const kind of [PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE, PROCUREMENT_RD_REQUEST_QUOTATION]) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
    }
  });

  it('plans the invoice-match read in Rust and runs it on the caller unit', async () => {
    const fetchMock = stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE], values: ['inv-1', 'company'] }] } });
    const { db } = fakeDb([{ id: 'bill-1', reference: 'INV-1' }]);
    const row = await nativeProcurementReadFirst<{ id: string; reference: string }>(
      PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE, ['inv-1', 'company'], db as never,
    );
    expect(row).toEqual({ id: 'bill-1', reference: 'INV-1' });
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(String(url)).toContain('/api/v1/holdings/acme/procurement/command-plan');
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('procurement_rd_invoice_match_finance');
    expect(body.values).toEqual(['inv-1', 'company']);
    expect(JSON.stringify(body)).not.toContain('SELECT');
  });

  it('switched finance entrypoint adapter carries no SQL and returns the planned row', async () => {
    stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE], values: ['inv-1', 'company'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'bill-1', reference: 'INV-1' }]);
    const row = await procurementInvoiceMatchForFinance(db as never, 'inv-1', 'company');
    expect(row).toEqual({ id: 'bill-1', reference: 'INV-1' });
    expect(prepared).toEqual([planned[PROCUREMENT_RD_INVOICE_MATCH_FOR_FINANCE]]);
    expect(bound).toEqual([['inv-1', 'company']]);
  });

  it('switched people-app entrypoint adapter carries no SQL and returns the planned row', async () => {
    stubFetch({ data: { statements: [{ sql: planned[PROCUREMENT_RD_REQUEST_QUOTATION], values: ['req-1', 'company'] }] } });
    const { db, prepared, bound } = fakeDb([{ id: 'rfq-1' }]);
    const row = await requestQuotation(db as never, 'req-1', 'company');
    expect(row).toEqual({ id: 'rfq-1' });
    expect(prepared).toEqual([planned[PROCUREMENT_RD_REQUEST_QUOTATION]]);
    expect(bound).toEqual([['req-1', 'company']]);
  });

  it('fails closed when the native transport is not configured', async () => {
    delete process.env[envKey];
    await expect(nativeProcurementReadAll(
      PROCUREMENT_RD_REQUEST_QUOTATION, ['req-1', 'company'], fakeDb().db as never,
    )).rejects.toMatchObject({ status: 503 });
  });
});
