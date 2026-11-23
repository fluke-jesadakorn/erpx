import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS,
} from '../../../packages/server-people_payroll/src/adapters/native-people-payroll-read';
import { RecordDetailsPeoplePayrollOwnedFacts } from '../../../packages/server-people_payroll/src/adapters/records-record-details-owned-facts';

const envKey = 'ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG';
const original = process.env[envKey];
beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44993',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = () => {
  const bound: unknown[][] = [];
  const prepared: string[] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
        async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
        async first() { return null; },
      };
    },
    async batch() { return [{ results: [{ id: 'run-1' }], success: true, meta: { changes: 0, duration: 0 } }]; },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared };
};

describe('native People Payroll caller-unit read plan', () => {
  it('exposes an allow-listed RD_* kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS).toBe('people_payroll_rd_record_supplement_rows');
    expect(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the record-supplement read and runs it on the caller unit with its binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    const result = await RecordDetailsPeoplePayrollOwnedFacts(db as never)
      .readRecordSupplementDataRows4('company', '2026-11', 'THB', 30);
    expect(result).toMatchObject({ results: [{ id: 'run-1' }] });

    expect(bodies).toEqual([{
      kind: PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS,
      values: ['company', '2026-11', 'THB', 30],
    }]);
    expect(bound).toEqual([['company', '2026-11', 'THB', 30]]);
    expect(prepared).toEqual(['SELECT 1 AS row']);

    const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/people-payroll/command-plan');
    expect((init.headers as Record<string, string>).origin).toBe('https://acme.example.com');
    expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe('company');
  });

  it('fails closed when configuration is missing or the plan is malformed', async () => {
    delete process.env[envKey];
    const { db, prepared } = fakeDb();
    await expect(nativePeoplePayrollReadAll(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS, ['company', '2026-11', 'THB', 0], db as never))
      .rejects.toMatchObject({ code: 'PEOPLE_PAYROLL_NATIVE_READ_UNAVAILABLE' });
    expect(prepared).toEqual([]);

    process.env[envKey] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44993', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: {} }), {
      status: 200, headers: { 'content-type': 'application/json' },
    })));
    await expect(nativePeoplePayrollReadAll(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS, ['company', '2026-11', 'THB', 0], db as never))
      .rejects.toMatchObject({ code: 'PEOPLE_PAYROLL_NATIVE_READ_UNAVAILABLE' });
  });
});
