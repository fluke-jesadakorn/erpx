import { readFile } from 'node:fs/promises';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePmsReadAll,
  nativePmsReadFirst,
  PMS_RD_INVENTORY_COMMAND,
  PMS_RD_BOOKING_CHANGE,
  PMS_RD_BOOKING_CHANGE_PENDING,
  PMS_RD_BOOKING_PAYMENT,
  PMS_RD_POSTING_CHARGE,
  PMS_RD_POSTING_BY_RECORD,
} from '../../../packages/server-pms/src/adapters/native-pms-read';
import { readPmsInventoryCommand } from '../../../packages/server-pms/src/adapters/pms/read-repository';

const envKey = 'ERPX_NATIVE_PMS_CONFIG';
const original = process.env[envKey];
const plannedSql = 'SELECT id,kind,data_json,version,updated_at FROM pms_records \
WHERE property_id=? AND kind IN (\'room_type\',\'room\',\'rate\',\'block\',\'item\',\'credit\') \
UNION ALL SELECT id,kind,data_json,version,updated_at FROM pms_records \
WHERE property_id=? AND kind=\'stay\' AND data_json::jsonb->>\'state\' IN (\'reserved\',\'in_house\') \
ORDER BY id';
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
    privateBaseUrl: 'http://127.0.0.1:44995',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = () => {
  const prepared: string[] = [];
  const bound: unknown[][] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
      };
    },
    async batch<T>() {
      return [{ results: [{ id: 'room-1', kind: 'room', data_json: '{}', version: 1, updated_at: 1 }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[];
    },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, prepared, bound };
};

describe('native PMS caller-unit read plan', () => {
  it('exposes an allow-listed PMS_RD_* kind with no SQL on the wire', () => {
    expect(PMS_RD_INVENTORY_COMMAND).toBe('pms_rd_inventory_command');
  });

  it('plans the owner read in Rust and runs it on the caller unit', async () => {
    const fetchMock = stubFetch({ data: { statements: [{ sql: plannedSql, values: ['property-1', 'property-1'] }] } });
    const { db, prepared, bound } = fakeDb();
    const rows = await nativePmsReadAll<{ id: string }>(PMS_RD_INVENTORY_COMMAND, ['property-1', 'property-1'], db as never);
    expect(rows.results[0]?.id).toBe('room-1');
    expect(prepared).toEqual([plannedSql]);
    expect(bound).toEqual([['property-1', 'property-1']]);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(String(url)).toContain('/api/v1/holdings/acme/pms/command-plan');
    const body = JSON.parse(init.body as string) as { kind: string; values: unknown[] };
    expect(body.kind).toBe('pms_rd_inventory_command');
    expect(body.values).toEqual(['property-1', 'property-1']);
    expect(JSON.stringify(body)).not.toContain('SELECT');
  });

  it('switched adapter carries no SQL and returns the planned rows', async () => {
    stubFetch({ data: { statements: [{ sql: plannedSql, values: ['property-1', 'property-1'] }] } });
    const { db, prepared } = fakeDb();
    const facts = await readPmsInventoryCommand(db as never, 'property-1');
    expect(facts).toHaveLength(1);
    expect(facts[0]?.id).toBe('room-1');
    expect(prepared).toEqual([plannedSql]);
  });

  it('fails closed when the native transport is not configured', async () => {
    delete process.env[envKey];
    await expect(nativePmsReadFirst(PMS_RD_INVENTORY_COMMAND, ['property-1', 'property-1'], fakeDb().db as never))
      .rejects.toMatchObject({ status: 503 });
  });
});

const changeSql = 'SELECT request_hash,state FROM pms_booking_changes WHERE id=? AND reservation_id=?';
const pendingSql = "SELECT id FROM pms_booking_changes WHERE reservation_id=? AND state='payment_pending' AND expires_at>? LIMIT 1";
const paymentSql = 'SELECT id FROM pms_booking_payments WHERE proposal_id=?';

describe('native PMS booking-change read retirement', () => {
  it('exposes fixed booking-change read kinds with no SQL on the wire', () => {
    expect(PMS_RD_BOOKING_CHANGE).toBe('pms_rd_booking_change');
    expect(PMS_RD_BOOKING_CHANGE_PENDING).toBe('pms_rd_booking_change_pending');
    expect(PMS_RD_BOOKING_PAYMENT).toBe('pms_rd_booking_payment');
    for (const kind of [PMS_RD_BOOKING_CHANGE, PMS_RD_BOOKING_CHANGE_PENDING, PMS_RD_BOOKING_PAYMENT])
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the three TypeScript booking-change SELECT literals but keeps refund/lock reads', async () => {
    const adapter = await readFile(new URL('../../../packages/server-pms/src/adapters/pms-booking-changes.ts', import.meta.url), 'utf8');
    expect(adapter).not.toContain(changeSql);
    expect(adapter).not.toContain(pendingSql);
    expect(adapter).not.toContain(paymentSql);
    expect(adapter).toContain('nativePmsReadFirst');
    expect(adapter).toContain('FOR UPDATE');
    expect(adapter).toContain('readPmsBookingRefundRequests');
  });

  it('plans change/pending/payment reads in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_BOOKING_CHANGE ? changeSql
        : body.kind === PMS_RD_BOOKING_CHANGE_PENDING ? pendingSql : paymentSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'payment-1', request_hash: 'hash', state: 'applied' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const change = await nativePmsReadFirst(PMS_RD_BOOKING_CHANGE, ['change-1', 'reservation-1'], db as never);
    const pending = await nativePmsReadFirst(PMS_RD_BOOKING_CHANGE_PENDING, ['reservation-1', 1000], db as never);
    const payment = await nativePmsReadFirst(PMS_RD_BOOKING_PAYMENT, ['change-1'], db as never);

    expect(change).toMatchObject({ request_hash: 'hash' });
    expect(pending).toMatchObject({ id: 'payment-1' });
    expect(payment).toMatchObject({ id: 'payment-1' });
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_BOOKING_CHANGE, PMS_RD_BOOKING_CHANGE_PENDING, PMS_RD_BOOKING_PAYMENT]);
    expect(bodies.map(b => b.values)).toEqual([['change-1', 'reservation-1'], ['reservation-1', 1000], ['change-1']]);
    expect(bound).toEqual([['change-1', 'reservation-1'], ['reservation-1', 1000], ['change-1']]);
    expect(prepared).toEqual([changeSql, pendingSql, paymentSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const chargeSql = "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'guestAccountId'=?) AND source_key LIKE '%:charge' ORDER BY business_date,source_key LIMIT 1";
const byRecordSql = 'SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id=? ORDER BY source_key LIMIT 1';

describe('native PMS posting-journal read retirement', () => {
  it('exposes fixed posting read kinds with no SQL on the wire', () => {
    expect(PMS_RD_POSTING_CHARGE).toBe('pms_rd_posting_charge');
    expect(PMS_RD_POSTING_BY_RECORD).toBe('pms_rd_posting_by_record');
    for (const kind of [PMS_RD_POSTING_CHARGE, PMS_RD_POSTING_BY_RECORD])
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the two TypeScript posting SELECT literals but keeps lock/refund reads', async () => {
    const adapter = await readFile(new URL('../../../packages/server-pms/src/adapters/pms.ts', import.meta.url), 'utf8');
    expect(adapter).not.toContain(chargeSql);
    expect(adapter).not.toContain(byRecordSql);
    expect(adapter).toContain('nativePmsReadFirst');
    expect(adapter).toContain('FOR UPDATE');
    expect(adapter).toContain('nativePmsOwnerWrite');
  });

  it('plans the posting reads in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_POSTING_CHARGE ? chargeSql : byRecordSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ journal_id: 'journal-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const charge = await nativePmsReadFirst(PMS_RD_POSTING_CHARGE, ['property-1', 'property-1', 'guest-1'], db as never);
    const byRecord = await nativePmsReadFirst(PMS_RD_POSTING_BY_RECORD, ['property-1', 'entry-1'], db as never);

    expect(charge).toMatchObject({ journal_id: 'journal-1' });
    expect(byRecord).toMatchObject({ journal_id: 'journal-1' });
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_POSTING_CHARGE, PMS_RD_POSTING_BY_RECORD]);
    expect(bodies.map(b => b.values)).toEqual([['property-1', 'property-1', 'guest-1'], ['property-1', 'entry-1']]);
    expect(bound).toEqual([['property-1', 'property-1', 'guest-1'], ['property-1', 'entry-1']]);
    expect(prepared).toEqual([chargeSql, byRecordSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});
