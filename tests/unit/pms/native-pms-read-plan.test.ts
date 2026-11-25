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
  PMS_RD_CHARGE_CORRECTION_PENDING,
  PMS_RD_CHARGE_CORRECTION_PENDING_COUNT,
  PMS_RD_GUEST_COMMAND_RECORD,
  PMS_RD_DOCUMENT_BY_ID,
  PMS_RD_DOCUMENT_SNAPSHOT,
  PMS_RD_PROPERTY_CONFIG,
  PMS_RD_PENDING_HOTEL_TRANSFERS,
  PMS_RD_UNPOSTED_BOOKING_RECEIPTS,
  PMS_RD_UNPOSTED_COUNTER_RECEIPTS,
  PMS_RD_HOSPITALITY_SETUP_PROPERTIES,
  PMS_RD_ATTACHMENT_PROPERTY_IMAGE,
  PMS_RD_ATTACHMENT_RECORD_IMAGE,
  PMS_RD_DOCUMENT_RELATED_POSTINGS,
  PMS_RD_POS_GUEST_HOTELS,
  PMS_RD_POS_GUEST_CHOICES,
  PMS_RD_POS_STRIPE_PAYMENT,
  PMS_RD_POS_BOOKING_PAYMENT,
  PMS_RD_STRIPE_PAYMENT_BY_ID,
  PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY,
  PMS_RD_STRIPE_PAYMENT_PROPERTY,
  PMS_RD_STAY_COMMAND,
  PMS_RD_CASHIER_COMMAND,
  PMS_RD_POS_COMMAND,
  PMS_RD_NIGHT_COMMAND,
} from '../../../packages/server-pms/src/adapters/native-pms-read';
import { readPmsInventoryCommand } from '../../../packages/server-pms/src/adapters/pms/read-repository';
import { pmsAttachmentReadSourceImage } from '../../../packages/server-pms/src/adapters/attachment-read-sources';

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

const ccPendingSql = "SELECT 1 FROM pms_charge_corrections WHERE entry_id=? AND status='pending'";
const ccPendingCountSql = "SELECT count(*) n FROM pms_charge_corrections WHERE company_id=? AND property_id=? AND status='pending'";

describe('native PMS charge-correction pending read retirement', () => {
  it('exposes fixed charge-correction pending read kinds with no SQL on the wire', () => {
    expect(PMS_RD_CHARGE_CORRECTION_PENDING).toBe('pms_rd_charge_correction_pending');
    expect(PMS_RD_CHARGE_CORRECTION_PENDING_COUNT).toBe('pms_rd_charge_correction_pending_count');
    for (const kind of [PMS_RD_CHARGE_CORRECTION_PENDING, PMS_RD_CHARGE_CORRECTION_PENDING_COUNT])
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the two TypeScript pending charge-correction SELECT literals but keeps the FOR UPDATE read', async () => {
    const adapter = await readFile(new URL('../../../packages/server-pms/src/adapters/pms.ts', import.meta.url), 'utf8');
    expect(adapter).not.toContain(ccPendingSql);
    expect(adapter).toContain('FOR UPDATE');
    expect(adapter).toContain('nativePmsReadFirst');
    const controls = await readFile(new URL('../../../packages/server-pms/src/adapters/hospitality-day-controls.ts', import.meta.url), 'utf8');
    expect(controls).not.toContain(ccPendingCountSql);
    expect(controls).toContain('nativePmsReadFirst');
    expect(controls).toContain('pendingHotelRoomChargeReversals');
  });

  it('plans the pending charge-correction reads in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_CHARGE_CORRECTION_PENDING ? ccPendingSql : ccPendingCountSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ '1': 1, n: 2 }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const pending = await nativePmsReadFirst(PMS_RD_CHARGE_CORRECTION_PENDING, ['entry-1'], db as never);
    const count = await nativePmsReadFirst<{ n: number }>(PMS_RD_CHARGE_CORRECTION_PENDING_COUNT, ['company-1', 'property-1'], db as never);

    expect(pending).toBeTruthy();
    expect(count).toMatchObject({ n: 2 });
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_CHARGE_CORRECTION_PENDING, PMS_RD_CHARGE_CORRECTION_PENDING_COUNT]);
    expect(bodies.map(b => b.values)).toEqual([['entry-1'], ['company-1', 'property-1']]);
    expect(bound).toEqual([['entry-1'], ['company-1', 'property-1']]);
    expect(prepared).toEqual([ccPendingSql, ccPendingCountSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const guestCommandRecordSql = "SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind='guest' AND id=?";

describe('native PMS guest command record read retirement', () => {
  it('exposes a fixed guest-command-record read kind with no SQL on the wire', () => {
    expect(PMS_RD_GUEST_COMMAND_RECORD).toBe('pms_rd_guest_command_record');
    expect(PMS_RD_GUEST_COMMAND_RECORD).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the TypeScript guest command record SELECT literal but keeps the scope UNION read', async () => {
    const adapter = await readFile(new URL('../../../packages/server-pms/src/adapters/pms.ts', import.meta.url), 'utf8');
    const call = adapter.split('\n').find(line => line.includes('readPmsGuestCommandRecord:')) ?? '';
    expect(call).toContain('nativePmsReadAll');
    expect(call).toContain('PMS_RD_GUEST_COMMAND_RECORD');
    expect(call).not.toContain('db.prepare');
    expect(adapter).toContain("UNION ALL ${PMS_RECORDS_PROJECTION_PREFIX_SQL} WHERE property_id=? AND kind='guest' AND id=?");
  });

  it('plans the guest command record read in Rust and runs it on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({ data: { statements: [{ sql: guestCommandRecordSql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'guest-1', kind: 'guest', data_json: '{}', version: 1, updated_at: 1 }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const rows = await nativePmsReadAll<{ id: string }>(PMS_RD_GUEST_COMMAND_RECORD, ['property-1', 'guest-1'], db as never);

    expect(rows.results[0]?.id).toBe('guest-1');
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_GUEST_COMMAND_RECORD]);
    expect(bodies.map(b => b.values)).toEqual([['property-1', 'guest-1']]);
    expect(bound).toEqual([['property-1', 'guest-1']]);
    expect(prepared).toEqual([guestCommandRecordSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const documentByIdSql = 'SELECT * FROM pms_documents WHERE id=?';
const documentSnapshotSql = 'SELECT property_id,snapshot_json FROM pms_documents WHERE id=?';

describe('native PMS document point-read retirement', () => {
  it('exposes fixed document read kinds with no SQL on the wire', () => {
    expect(PMS_RD_DOCUMENT_BY_ID).toBe('pms_rd_document_by_id');
    expect(PMS_RD_DOCUMENT_SNAPSHOT).toBe('pms_rd_document_snapshot');
    for (const kind of [PMS_RD_DOCUMENT_BY_ID, PMS_RD_DOCUMENT_SNAPSHOT]) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
    }
  });

  it('removes both TypeScript pms_documents point-read SELECT literals', async () => {
    const entrypoint = await readFile(new URL('../../../packages/server-pms/src/adapters/entrypoint-queries.ts', import.meta.url), 'utf8');
    const document = entrypoint.split('\n').find(line => line.includes('document: (id: string)')) ?? '';
    expect(document).toContain('nativePmsReadFirst');
    expect(document).toContain('PMS_RD_DOCUMENT_BY_ID');
    expect(document).not.toContain('db.prepare');
    expect(entrypoint).not.toContain('SELECT * FROM pms_documents WHERE id=?');

    const details = await readFile(new URL('../../../packages/server-pms/src/adapters/pms/record-details.ts', import.meta.url), 'utf8');
    expect(details).toContain('PMS_RD_DOCUMENT_SNAPSHOT');
    expect(details).not.toContain('SELECT property_id,snapshot_json FROM pms_documents WHERE id=?');
  });

  it('plans both document reads in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_DOCUMENT_BY_ID ? documentByIdSql : documentSnapshotSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ property_id: 'property-1', snapshot_json: '{}' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const full = await nativePmsReadFirst<{ property_id: string }>(PMS_RD_DOCUMENT_BY_ID, ['doc-1'], db as never);
    const snapshot = await nativePmsReadFirst<{ property_id: string }>(PMS_RD_DOCUMENT_SNAPSHOT, ['doc-1'], db as never);

    expect(full).toMatchObject({ property_id: 'property-1' });
    expect(snapshot).toMatchObject({ property_id: 'property-1' });
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_DOCUMENT_BY_ID, PMS_RD_DOCUMENT_SNAPSHOT]);
    expect(bodies.map(b => b.values)).toEqual([['doc-1'], ['doc-1']]);
    expect(bound).toEqual([['doc-1'], ['doc-1']]);
    expect(prepared).toEqual([documentByIdSql, documentSnapshotSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const propertyConfigSql = 'SELECT config_json FROM pms_properties WHERE id=? AND company_id=?';

describe('native PMS property config read retirement', () => {
  it('exposes a fixed property-config read kind with no SQL on the wire', () => {
    expect(PMS_RD_PROPERTY_CONFIG).toBe('pms_rd_property_config');
    expect(PMS_RD_PROPERTY_CONFIG).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the TypeScript pms_properties config SELECT literal', async () => {
    const controls = await readFile(new URL('../../../packages/server-pms/src/adapters/hospitality-day-controls.ts', import.meta.url), 'utf8');
    expect(controls).not.toContain(propertyConfigSql);
    expect(controls).toContain('nativePmsReadFirst');
    expect(controls).toContain('PMS_RD_PROPERTY_CONFIG');
  });

  it('plans the property config read in Rust and runs it on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({ data: { statements: [{ sql: propertyConfigSql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ config_json: '{}' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const row = await nativePmsReadFirst<{ config_json: string }>(PMS_RD_PROPERTY_CONFIG, ['property-1', 'company-1'], db as never);

    expect(row).toMatchObject({ config_json: '{}' });
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_PROPERTY_CONFIG]);
    expect(bodies.map(b => b.values)).toEqual([['property-1', 'company-1']]);
    expect(bound).toEqual([['property-1', 'company-1']]);
    expect(prepared).toEqual([propertyConfigSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const pendingTransfersSql = "SELECT count(*) n FROM pms_transfer_requests WHERE company_id=? AND property_id=? AND business_date=? AND state='pending'";
const unpostedBookingReceiptsSql = "SELECT count(*) n FROM pms_booking_payments\n    WHERE property_id=? AND company_id=? AND paid_at IS NOT NULL AND status IN ('payment_pending','expired')\n    AND to_char(to_timestamp(paid_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?";

describe('native PMS day-control count-read retirement', () => {
  it('exposes fixed count read kinds with no SQL on the wire', () => {
    expect(PMS_RD_PENDING_HOTEL_TRANSFERS).toBe('pms_rd_pending_hotel_transfers');
    expect(PMS_RD_UNPOSTED_BOOKING_RECEIPTS).toBe('pms_rd_unposted_booking_receipts');
    for (const kind of [PMS_RD_PENDING_HOTEL_TRANSFERS, PMS_RD_UNPOSTED_BOOKING_RECEIPTS]) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
    }
  });

  it('removes both TypeScript day-control count SELECT literals', async () => {
    const controls = await readFile(new URL('../../../packages/server-pms/src/adapters/hospitality-day-controls.ts', import.meta.url), 'utf8');
    expect(controls).not.toContain('SELECT count(*) n FROM pms_transfer_requests');
    expect(controls).not.toContain('FROM pms_booking_payments');
    expect(controls).toContain('PMS_RD_PENDING_HOTEL_TRANSFERS');
    expect(controls).toContain('PMS_RD_UNPOSTED_BOOKING_RECEIPTS');
  });

  it('plans both count reads in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_PENDING_HOTEL_TRANSFERS ? pendingTransfersSql : unpostedBookingReceiptsSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ n: 2 }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const transfers = await nativePmsReadFirst<{ n: number }>(PMS_RD_PENDING_HOTEL_TRANSFERS, ['company-1', 'property-1', '2026-11-24'], db as never);
    const receipts = await nativePmsReadFirst<{ n: number }>(PMS_RD_UNPOSTED_BOOKING_RECEIPTS, ['property-1', 'company-1', 'Asia/Bangkok', '2026-11-24'], db as never);

    expect(transfers).toMatchObject({ n: 2 });
    expect(receipts).toMatchObject({ n: 2 });
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_PENDING_HOTEL_TRANSFERS, PMS_RD_UNPOSTED_BOOKING_RECEIPTS]);
    expect(bodies.map(b => b.values)).toEqual([['company-1', 'property-1', '2026-11-24'], ['property-1', 'company-1', 'Asia/Bangkok', '2026-11-24']]);
    expect(bound).toEqual([['company-1', 'property-1', '2026-11-24'], ['property-1', 'company-1', 'Asia/Bangkok', '2026-11-24']]);
    expect(prepared).toEqual([pendingTransfersSql, unpostedBookingReceiptsSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const unpostedCounterReceiptsSql = "SELECT count(*) n FROM pms_stripe_payments\n    WHERE property_id=? AND company_id=? AND received_at IS NOT NULL AND state IN ('requested','pending','cancelled')\n    AND to_char(to_timestamp(received_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?";

describe('native PMS unposted counter receipts read retirement', () => {
  it('exposes a fixed unposted-counter-receipts read kind with no SQL on the wire', () => {
    expect(PMS_RD_UNPOSTED_COUNTER_RECEIPTS).toBe('pms_rd_unposted_counter_receipts');
    expect(PMS_RD_UNPOSTED_COUNTER_RECEIPTS).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the TypeScript unposted counter receipts SELECT literal but keeps refund reads', async () => {
    const controls = await readFile(new URL('../../../packages/server-pms/src/adapters/hospitality-day-controls.ts', import.meta.url), 'utf8');
    expect(controls).not.toContain('FROM pms_stripe_payments\n    WHERE property_id=? AND company_id=? AND received_at IS NOT NULL');
    expect(controls).toContain('PMS_RD_UNPOSTED_COUNTER_RECEIPTS');
    const call = controls.split('\n').find(line => line.includes('PMS_RD_UNPOSTED_COUNTER_RECEIPTS,')) ?? '';
    expect(call).toContain('nativePmsReadFirst');
    expect(call).not.toContain('db.prepare');
    expect(controls).toContain("refund_state='failed'");
  });

  it('plans the unposted counter receipts read in Rust and runs it on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({ data: { statements: [{ sql: unpostedCounterReceiptsSql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ n: 3 }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const receipts = await nativePmsReadFirst<{ n: number }>(PMS_RD_UNPOSTED_COUNTER_RECEIPTS, ['property-1', 'company-1', 'Asia/Bangkok', '2026-11-24'], db as never);

    expect(receipts).toMatchObject({ n: 3 });
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_UNPOSTED_COUNTER_RECEIPTS]);
    expect(bodies.map(b => b.values)).toEqual([['property-1', 'company-1', 'Asia/Bangkok', '2026-11-24']]);
    expect(bound).toEqual([['property-1', 'company-1', 'Asia/Bangkok', '2026-11-24']]);
    expect(prepared).toEqual([unpostedCounterReceiptsSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const hospitalitySetupPropertiesSql = 'SELECT id,name,state,business_date FROM pms_properties WHERE company_id=? AND branch_id=? ORDER BY name';

describe('native PMS hospitality-setup property list read retirement', () => {
  it('exposes a fixed property-list read kind with no SQL on the wire', () => {
    expect(PMS_RD_HOSPITALITY_SETUP_PROPERTIES).toBe('pms_rd_hospitality_setup_properties');
    expect(PMS_RD_HOSPITALITY_SETUP_PROPERTIES).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the TypeScript hospitality-setup pms_properties SELECT literal', async () => {
    const setup = await readFile(new URL('../../../packages/server-pms/src/adapters/hospitality-setup.ts', import.meta.url), 'utf8');
    expect(setup).not.toContain(hospitalitySetupPropertiesSql);
    expect(setup).toContain('nativePmsReadAll');
    expect(setup).toContain('PMS_RD_HOSPITALITY_SETUP_PROPERTIES');
    const call = setup.split('\n').find(line => line.includes('PMS_RD_HOSPITALITY_SETUP_PROPERTIES,')) ?? '';
    expect(call).toContain('nativePmsReadAll');
    expect(call).not.toContain('db.prepare');
  });

  it('plans the property-list read in Rust and runs it on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({ data: { statements: [{ sql: hospitalitySetupPropertiesSql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'property-1', name: 'Grand', state: 'open', business_date: '2026-11-24' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const rows = await nativePmsReadAll<{ id: string }>(PMS_RD_HOSPITALITY_SETUP_PROPERTIES, ['company-1', 'branch-1'], db as never);

    expect(rows.results).toMatchObject([{ id: 'property-1', name: 'Grand', state: 'open', business_date: '2026-11-24' }]);
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_HOSPITALITY_SETUP_PROPERTIES]);
    expect(bodies.map(b => b.values)).toEqual([['company-1', 'branch-1']]);
    expect(bound).toEqual([['company-1', 'branch-1']]);
    expect(prepared).toEqual([hospitalitySetupPropertiesSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const attachmentPropertyImageSql = 'SELECT to_jsonb(p)::text image FROM pms_properties p WHERE p.id=? AND p.company_id=?';
const attachmentRecordImageSql = "SELECT jsonb_build_object('property',to_jsonb(p),'record',to_jsonb(r))::text image FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND p.company_id=?";

describe('native PMS attachment read-source image retirement', () => {
  it('exposes fixed attachment read kinds with no SQL on the wire', () => {
    expect(PMS_RD_ATTACHMENT_PROPERTY_IMAGE).toBe('pms_rd_attachment_property_image');
    expect(PMS_RD_ATTACHMENT_RECORD_IMAGE).toBe('pms_rd_attachment_record_image');
    for (const kind of [PMS_RD_ATTACHMENT_PROPERTY_IMAGE, PMS_RD_ATTACHMENT_RECORD_IMAGE])
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes both TypeScript attachment image SELECT literals but keeps the source lease', async () => {
    const adapter = await readFile(new URL('../../../packages/server-pms/src/adapters/attachment-read-sources.ts', import.meta.url), 'utf8');
    expect(adapter).not.toContain(attachmentPropertyImageSql);
    expect(adapter).not.toContain(attachmentRecordImageSql);
    expect(adapter).toContain('nativePmsReadFirst');
    expect(adapter).toContain('PMS_RD_ATTACHMENT_PROPERTY_IMAGE');
    expect(adapter).toContain('PMS_RD_ATTACHMENT_RECORD_IMAGE');
    expect(adapter).toContain('LOCK TABLE pms_properties,pms_records IN SHARE ROW EXCLUSIVE MODE');
  });

  it('plans both attachment images in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_ATTACHMENT_PROPERTY_IMAGE ? attachmentPropertyImageSql : attachmentRecordImageSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ image: '{"snapshot":true}' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const propertyImage = await pmsAttachmentReadSourceImage(db as never, 'company-1', 'pms_property', 'property-1');
    const recordImage = await pmsAttachmentReadSourceImage(db as never, 'company-1', 'pms_guest', 'guest-1');

    expect(propertyImage).toBe('{"snapshot":true}');
    expect(recordImage).toBe('{"snapshot":true}');
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_ATTACHMENT_PROPERTY_IMAGE, PMS_RD_ATTACHMENT_RECORD_IMAGE]);
    expect(bodies.map(b => b.values)).toEqual([['property-1', 'company-1'], ['guest-1', 'company-1']]);
    expect(bound).toEqual([['property-1', 'company-1'], ['guest-1', 'company-1']]);
    expect(prepared).toEqual([attachmentPropertyImageSql, attachmentRecordImageSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const relatedPostingsSql = "SELECT DISTINCT pp.journal_id FROM pms_postings pp JOIN pms_records r ON r.id=pp.record_id AND r.property_id=pp.property_id\n    WHERE pp.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'guestAccountId'=(SELECT guest_account_id FROM pms_documents WHERE id=?)";

describe('native PMS document related-postings read retirement', () => {
  it('exposes a fixed related-postings read kind with no SQL on the wire', () => {
    expect(PMS_RD_DOCUMENT_RELATED_POSTINGS).toBe('pms_rd_document_related_postings');
    expect(PMS_RD_DOCUMENT_RELATED_POSTINGS).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('removes the TypeScript related-postings SELECT literal', async () => {
    const entrypoint = await readFile(new URL('../../../packages/server-pms/src/adapters/entrypoint-queries.ts', import.meta.url), 'utf8');
    const call = entrypoint.split('\n').find(line => line.includes('PMS_RD_DOCUMENT_RELATED_POSTINGS,')) ?? '';
    expect(call).toContain('nativePmsReadAll');
    expect(call).not.toContain('db.prepare');
    expect(entrypoint).not.toContain(relatedPostingsSql);
    expect(entrypoint).toContain('PMS_RD_DOCUMENT_BY_ID');
  });

  it('plans the related-postings read in Rust and runs it on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({ data: { statements: [{ sql: relatedPostingsSql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ journal_id: 'journal-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const { pmsDocumentQueries } = await import('../../../packages/server-pms/src/adapters/entrypoint-queries');
    const postings = await pmsDocumentQueries(db as never).relatedPostings('property-1', 'doc-1');

    expect(postings).toMatchObject([{ journal_id: 'journal-1' }]);
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_DOCUMENT_RELATED_POSTINGS]);
    expect(bodies.map(b => b.values)).toEqual([['property-1', 'doc-1']]);
    expect(bound).toEqual([['property-1', 'doc-1']]);
    expect(prepared).toEqual([relatedPostingsSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const posGuestHotelsSql = 'SELECT id,name,branch_id FROM pms_properties WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY name,id LIMIT 100';
const posGuestChoicesSql = "SELECT p.id property_id,p.version property_version,s.id stay_id,\n        r.data_json::jsonb->>'number' room_number,g.data_json::jsonb->>'name' guest_name\n        FROM pms_properties p JOIN pms_records s ON s.property_id=p.id AND s.kind='stay'\n        JOIN pms_records a ON a.id=s.data_json::jsonb->>'guestAccountId' AND a.property_id=p.id AND a.kind='guest_account'\n        JOIN pms_records r ON r.id=s.data_json::jsonb->>'roomId' AND r.property_id=p.id AND r.kind='room'\n        JOIN pms_records g ON g.id=s.data_json::jsonb->>'guestId' AND g.property_id=p.id AND g.kind='guest'\n        WHERE p.company_id=? AND p.branch_id=? AND p.id=? AND p.state='open' AND p.config_json::jsonb->>'currency'=?\n        AND s.data_json::jsonb->>'state'='in_house' AND a.data_json::jsonb->>'state'='open'\n        AND COALESCE(a.data_json::jsonb->>'invoiceId','')='' ORDER BY r.data_json::jsonb->>'number',s.id LIMIT 100";

describe('native PMS pos-guest-account read retirement', () => {
  it('exposes fixed pos-guest read kinds with no SQL on the wire', () => {
    expect(PMS_RD_POS_GUEST_HOTELS).toBe('pms_rd_pos_guest_hotels');
    expect(PMS_RD_POS_GUEST_CHOICES).toBe('pms_rd_pos_guest_choices');
    for (const kind of [PMS_RD_POS_GUEST_HOTELS, PMS_RD_POS_GUEST_CHOICES])
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE|FOR SHARE/);
  });

  it('removes the two TypeScript pos-guest SELECT literals but keeps assertDay FOR SHARE', async () => {
    const adapterUrl = new URL('../../../packages/server-pms/src/adapters/pos-guest-account-interface.ts', import.meta.url);
    const adapter = await readFile(adapterUrl, 'utf8');
    expect(adapter).not.toContain(posGuestHotelsSql);
    expect(adapter).not.toContain(posGuestChoicesSql);
    expect(adapter).toContain('nativePmsReadAll');
    expect(adapter).toContain('PMS_RD_POS_GUEST_HOTELS');
    expect(adapter).toContain('PMS_RD_POS_GUEST_CHOICES');
    const hotels = adapter.split('\n').find(line => line.includes('PMS_RD_POS_GUEST_HOTELS,')) ?? '';
    const choices = adapter.split('\n').find(line => line.includes('PMS_RD_POS_GUEST_CHOICES,')) ?? '';
    for (const call of [hotels, choices]) {
      expect(call).toContain('nativePmsReadAll');
      expect(call).not.toContain('db.prepare');
    }
    // The hotel day assert keeps its locking read in the caller's TS unit.
    expect(adapter).toContain('FOR SHARE');
    expect(adapter).toContain('POS_HOTEL_DAY_CLOSED');
  });

  it('plans both reads in Rust and runs them on the caller unit with the same binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_POS_GUEST_HOTELS ? posGuestHotelsSql : posGuestChoicesSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'property-1', name: 'Grand', branch_id: 'branch-1', property_id: 'property-1', stay_id: 'stay-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const { posGuestAccountInterface } = await import('../../../packages/server-pms/src/adapters/pos-guest-account-interface');
    const api = posGuestAccountInterface(db as never);
    const hotels = await api.hotels('company-1', 'branch-1');
    const choices = await api.choices('company-1', 'branch-1', 'THB', 'property-1');

    expect(hotels).toMatchObject([{ id: 'property-1' }]);
    expect(choices).toMatchObject([{ property_id: 'property-1', stay_id: 'stay-1' }]);
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_POS_GUEST_HOTELS, PMS_RD_POS_GUEST_CHOICES]);
    expect(bodies.map(b => b.values)).toEqual([
      ['company-1', 'branch-1', 'branch-1'],
      ['company-1', 'branch-1', 'property-1', 'THB'],
    ]);
    expect(bound).toEqual(bodies.map(b => b.values));
    expect(prepared).toEqual([posGuestHotelsSql, posGuestChoicesSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const posStripePaymentSql = 'SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?';
const posBookingPaymentSql = 'SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=?';

describe('native PMS pos-stripe byte-identical read retirement', () => {
  it('exposes fixed pos-stripe fact read kinds with no SQL on the wire', () => {
    expect(PMS_RD_POS_STRIPE_PAYMENT).toBe('pms_rd_pos_stripe_payment');
    expect(PMS_RD_POS_BOOKING_PAYMENT).toBe('pms_rd_pos_booking_payment');
    for (const kind of [PMS_RD_POS_STRIPE_PAYMENT, PMS_RD_POS_BOOKING_PAYMENT])
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE/);
  });

  it('removes the four byte-identical TypeScript SELECT literals but keeps refund reads', async () => {
    const adapter = await readFile(new URL('../../../packages/server-pms/src/adapters/pos-stripe-facts.ts', import.meta.url), 'utf8');
    expect(adapter).not.toContain("one(db,'" + posStripePaymentSql + "'");
    expect(adapter).not.toContain("one(db,'" + posBookingPaymentSql + "'");
    expect(adapter).toContain('nativePmsReadFirst');
    expect(adapter).toContain('PMS_RD_POS_STRIPE_PAYMENT');
    expect(adapter).toContain('PMS_RD_POS_BOOKING_PAYMENT');
    for (const method of ['readPmsStripePayments1', 'readPmsStripePayments2'])
      expect(adapter).toMatch(new RegExp(method + '[^\\n]*PMS_RD_POS_STRIPE_PAYMENT'));
    for (const method of ['readPmsBookingPayments1', 'readPmsBookingPayments2'])
      expect(adapter).toMatch(new RegExp(method + '[^\\n]*PMS_RD_POS_BOOKING_PAYMENT'));
    // Dedicated refund-state reads and lock clauses stay in the caller's TS unit.
    expect(adapter).toContain('readPmsBookingRefundRequests1');
    expect(adapter).toContain("refund_id=?");
    expect(adapter).not.toContain('FOR UPDATE');
    expect(adapter).not.toContain('FOR SHARE');
  });

  it('plans both byte-identical reads in Rust and runs them on the caller unit with the same binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_POS_STRIPE_PAYMENT ? posStripePaymentSql : posBookingPaymentSql;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'payment-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const { posStripePmsFacts } = await import('../../../packages/server-pms/src/adapters/pos-stripe-facts');
    const api = posStripePmsFacts(db as never);
    const facts = { company_id: 'company-1', account_id: 'acct_1', livemode: false };
    const stripe1 = await api.readPmsStripePayments1({ counterId: 'counter-1', ...facts });
    const booking1 = await api.readPmsBookingPayments1({ bookingId: 'booking-1', ...facts });
    const stripe2 = await api.readPmsStripePayments2({ counterId: 'counter-1', ...facts });
    const booking2 = await api.readPmsBookingPayments2({ bookingId: 'booking-1', ...facts });

    for (const row of [stripe1, booking1, stripe2, booking2]) expect(row).toMatchObject({ id: 'payment-1' });
    expect(bodies.map(b => b.kind)).toEqual([
      PMS_RD_POS_STRIPE_PAYMENT, PMS_RD_POS_BOOKING_PAYMENT,
      PMS_RD_POS_STRIPE_PAYMENT, PMS_RD_POS_BOOKING_PAYMENT,
    ]);
    expect(bodies.map(b => b.values)).toEqual([
      ['counter-1', 'company-1', 'acct_1', false],
      ['booking-1', 'company-1', 'acct_1', false],
      ['counter-1', 'company-1', 'acct_1', false],
      ['booking-1', 'company-1', 'acct_1', false],
    ]);
    expect(bound).toEqual(bodies.map(b => b.values));
    expect(prepared).toEqual([
      posStripePaymentSql, posBookingPaymentSql, posStripePaymentSql, posBookingPaymentSql,
    ]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});

const stripePaymentByIdSql = 'SELECT * FROM pms_stripe_payments WHERE id=?';
const stripePaymentByIdCompanySql = 'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=?';
const stripePaymentPropertySql = 'SELECT property_id FROM pms_stripe_payments WHERE id=? AND company_id=?';
const stripePlainKindSql: Record<string, string> = {
  [PMS_RD_STRIPE_PAYMENT_BY_ID]: stripePaymentByIdSql,
  [PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY]: stripePaymentByIdCompanySql,
  [PMS_RD_STRIPE_PAYMENT_PROPERTY]: stripePaymentPropertySql,
};

describe('native PMS pms-stripe byte-identical plain read retirement', () => {
  it('exposes fixed payment point read kinds with no SQL on the wire', () => {
    expect(PMS_RD_STRIPE_PAYMENT_BY_ID).toBe('pms_rd_stripe_payment_by_id');
    expect(PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY).toBe('pms_rd_stripe_payment_by_id_company');
    expect(PMS_RD_STRIPE_PAYMENT_PROPERTY).toBe('pms_rd_stripe_payment_property');
    for (const kind of Object.keys(stripePlainKindSql))
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|REFUND/);
  });

  it('removes the eight byte-identical TypeScript SELECT literals but keeps lock and refund reads', async () => {
    const adapter = await readFile(new URL('../../../packages/server-pms/src/adapters/pms-stripe.ts', import.meta.url), 'utf8');
    expect(adapter).not.toContain("db.prepare('" + stripePaymentByIdSql + "')");
    expect(adapter).not.toContain("db.prepare('" + stripePaymentByIdCompanySql + "')");
    expect(adapter).not.toContain("db.prepare('" + stripePaymentPropertySql + "')");
    expect(adapter).toContain('nativePmsReadFirst');
    const routesTo = (method: string, kind: string) =>
      expect(adapter).toMatch(new RegExp(method + ':\\s*\\(facts:[\\s\\S]{0,200}?nativePmsReadFirst[^\\n]{0,80}?' + kind + ','));
    for (const method of ['readPmsStripePayments2', 'readPmsStripePayments4', 'readPmsStripePayments5', 'readPmsStripePayments13'])
      routesTo(method, 'PMS_RD_STRIPE_PAYMENT_BY_ID');
    for (const method of ['readPmsStripePayments3', 'readPmsStripePayments16'])
      routesTo(method, 'PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY');
    for (const method of ['readPmsStripePayments9', 'readPmsStripePayments11'])
      routesTo(method, 'PMS_RD_STRIPE_PAYMENT_PROPERTY');
    // Lock clauses and refund-state reads stay in the caller's TypeScript unit.
    expect(adapter).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(adapter).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect(adapter).toContain("refund_state IN ('approved','processing','requires_action')");
    expect(adapter).toContain('refund_state IS NOT NULL');
  });

  it('plans the three byte-identical reads in Rust and runs them on the caller unit with the same binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = stripePlainKindSql[body.kind];
      expect(sql).toBeTruthy();
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'payment-1', property_id: 'property-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const byId = await nativePmsReadFirst(PMS_RD_STRIPE_PAYMENT_BY_ID, ['payment-1'], db as never);
    const byCompany = await nativePmsReadFirst(PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY, ['payment-1', 'company-1'], db as never);
    const property = await nativePmsReadFirst(PMS_RD_STRIPE_PAYMENT_PROPERTY, ['payment-1', 'company-1'], db as never);

    for (const row of [byId, byCompany, property]) expect(row).toMatchObject({ id: 'payment-1' });
    expect(bodies.map(b => b.kind)).toEqual([
      PMS_RD_STRIPE_PAYMENT_BY_ID, PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY, PMS_RD_STRIPE_PAYMENT_PROPERTY,
    ]);
    expect(bodies.map(b => b.values)).toEqual([
      ['payment-1'],
      ['payment-1', 'company-1'],
      ['payment-1', 'company-1'],
    ]);
    expect(bound).toEqual(bodies.map(b => b.values));
    expect(prepared).toEqual([stripePaymentByIdSql, stripePaymentByIdCompanySql, stripePaymentPropertySql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });
});


const stayCommandSql = "WITH scope AS (\n          SELECT ?::text property_id, ?::text target_id, ?::text target_kind,\n            ?::text guest_id, ?::text account_id, ?::text room_id\n        ), target AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind=q.target_kind AND r.id=q.target_id\n        ), booking_ids AS (\n          SELECT CASE WHEN kind='reservation' THEN id ELSE data_json::jsonb->>'reservationId' END id FROM target\n        ), siblings AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind='stay'\n            AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)\n        ), account_candidates AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind='guest_account'\n            AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account'\n              AND r.id IN (SELECT data_json::jsonb->>'guestAccountId' FROM siblings)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id=q.account_id\n        ), accounts AS MATERIALIZED (\n          SELECT * FROM account_candidates\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.data_json::jsonb->>'state'='credit'\n              AND r.data_json::jsonb->>'partyId' IN (SELECT data_json::jsonb->>'partyId' FROM account_candidates WHERE data_json::jsonb->>'state'='credit')\n        ), affected_rooms AS (\n          SELECT data_json::jsonb->>'roomId' id FROM target WHERE kind='stay'\n          UNION SELECT room_id FROM scope\n        ), selected AS (\n          SELECT * FROM target\n          UNION SELECT * FROM siblings\n          UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('room_type','room','rate','block','item','credit')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('reserved','in_house')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='reservation' AND r.id IN (SELECT id FROM booking_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund') AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest' AND (r.id=q.guest_id\n              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM siblings)\n              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts)\n              OR r.id IN (SELECT data_json::jsonb->>'guestId' FROM pms_records WHERE property_id=q.property_id AND kind='reservation' AND id IN (SELECT id FROM booking_ids)))\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='task' AND r.data_json::jsonb->>'roomId' IN (SELECT id FROM affected_rooms)\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const cashierCommandSql = "WITH scope AS (\n          SELECT ?::text property_id, ?::text target_id, ?::text target_kind, ?::text shift_id, ?::text user_id\n        ), target AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind=CASE WHEN q.target_kind='account_close' THEN 'guest_account' ELSE q.target_kind END AND r.id=q.target_id\n        ), account_ids AS (\n          SELECT CASE WHEN kind='refund' THEN data_json::jsonb->>'guestAccountId' ELSE id END id\n          FROM target WHERE kind IN ('guest_account','refund')\n        ), accounts AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n          WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)\n        ), selected AS (\n          SELECT * FROM target\n          UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('rate','credit','item')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest'\n              AND r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND q.target_kind='account_close' AND r.kind='stay'\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='shift' AND (r.id=q.shift_id OR\n              (q.target_kind='new_shift' AND r.data_json::jsonb->>'userId'=q.user_id AND r.data_json::jsonb->>'state'='open'))\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const posCommandSql = "WITH scope AS (\n          SELECT ?::text property_id,?::text stay_id,?::text source_key\n        ), source AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey'=q.source_key\n        ), stays AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.id=q.stay_id\n        ), account_ids AS (\n          SELECT data_json::jsonb->>'guestAccountId' id FROM source\n          UNION SELECT data_json::jsonb->>'guestAccountId' id FROM stays\n        ), accounts AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)\n        ), selected AS (\n          SELECT * FROM source UNION SELECT * FROM stays UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='entry'\n              AND r.data_json::jsonb->>'originalId' IN (SELECT id FROM source)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest'\n              AND r.id IN (SELECT data_json::jsonb->>'guestId' FROM accounts UNION SELECT data_json::jsonb->>'guestId' FROM stays)\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const nightCommandSql = "WITH scope AS (\n          SELECT ?::text property_id,?::text business_date,?::text target_id\n        ), stays AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('reserved','in_house')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='stay'\n              AND r.data_json::jsonb->>'departure'>q.business_date AND r.data_json::jsonb->>'arrival'<=q.business_date\n        ), accounts AS MATERIALIZED (\n          SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest_account'\n              AND r.id IN (SELECT data_json::jsonb->>'guestAccountId' FROM stays)\n        ), selected AS (\n          SELECT * FROM stays UNION SELECT * FROM accounts\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('room_type','room','rate','block','item','credit')\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind IN ('entry','refund')\n              AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='guest' AND r.id IN (\n              SELECT data_json::jsonb->>'guestId' FROM stays UNION SELECT data_json::jsonb->>'guestId' FROM accounts)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='night' AND (r.id=q.target_id OR r.data_json::jsonb->>'date'=q.business_date)\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='shift' AND (\n              r.data_json::jsonb->>'state'='open' OR (r.data_json::jsonb->>'state'='closed' AND r.data_json::jsonb->'variance' IS DISTINCT FROM '0'::jsonb))\n          UNION SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q\n            WHERE r.property_id=q.property_id AND r.kind='task' AND r.data_json::jsonb->>'type'='stayover'\n              AND r.data_json::jsonb->>'date'=(q.business_date::date+1)::text\n              AND r.data_json::jsonb->>'roomId' IN (SELECT data_json::jsonb->>'roomId' FROM stays)\n        ) SELECT * FROM selected ORDER BY updated_at,id";
const scopedKindSql: Record<string, string> = {
  [PMS_RD_STAY_COMMAND]: stayCommandSql,
  [PMS_RD_CASHIER_COMMAND]: cashierCommandSql,
  [PMS_RD_POS_COMMAND]: posCommandSql,
  [PMS_RD_NIGHT_COMMAND]: nightCommandSql,
};

describe('native PMS scoped-records read retirement', () => {
  it('exposes fixed scoped-records read kinds with no SQL on the wire', () => {
    expect(PMS_RD_STAY_COMMAND).toBe('pms_rd_stay_command');
    expect(PMS_RD_CASHIER_COMMAND).toBe('pms_rd_cashier_command');
    expect(PMS_RD_POS_COMMAND).toBe('pms_rd_pos_command');
    expect(PMS_RD_NIGHT_COMMAND).toBe('pms_rd_night_command');
    for (const kind of Object.keys(scopedKindSql))
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });

  it('removes the four TypeScript scoped literals but keeps every lock read inline', async () => {
    const root = new URL('../../../packages/server-pms/src/adapters/', import.meta.url);
    const pms = await readFile(new URL('pms.ts', root), 'utf8');
    expect(pms).not.toContain('WITH scope AS (');
    expect(pms).not.toContain('PMS_SCOPED_RECORDS_SQL');
    expect(pms).not.toContain('PMS_RESERVATION_ID_MEMBER_SQL');
    expect(pms).not.toContain('PMS_GUEST_ACCOUNT_CASHIER_MEMBER_SQL');
    expect(pms).not.toContain('PMS_GUEST_ID_FROM_STAYS_SQL');
    for (const [name, kind] of [
      ['readPmsStayCommand', 'PMS_RD_STAY_COMMAND'],
      ['readPmsCashierCommand', 'PMS_RD_CASHIER_COMMAND'],
      ['readPmsPosCommand', 'PMS_RD_POS_COMMAND'],
      ['readPmsNightCommand', 'PMS_RD_NIGHT_COMMAND'],
    ] as const) {
      expect(pms).toMatch(new RegExp(name + ':[\\s\\S]{0,180}?nativePmsReadAll<[^>]+>\\(' + kind + ','));
    }
    // Locking reads stay inline in the caller's TypeScript unit.
    expect(pms).toContain('FOR UPDATE OF p');
    expect(await readFile(new URL('pms-stripe.ts', root), 'utf8')).toContain('FOR UPDATE');
    expect(await readFile(new URL('feature-availability.ts', root), 'utf8')).toContain('FOR SHARE');
    expect(await readFile(new URL('attachment-read-sources.ts', root), 'utf8')).toContain('LOCK TABLE');
  });

  it('carries byte-identical retired SQL in the Rust registry', async () => {
    const rust = await readFile(new URL('../../../packages/server-pms/src/adapters/native_read_sql.rs', import.meta.url), 'utf8');
    for (const [suffix, sql] of [
      ['STAY', stayCommandSql], ['CASHIER', cashierCommandSql],
      ['POS', posCommandSql], ['NIGHT', nightCommandSql],
    ] as const) {
      expect(rust).toContain('const SQL_' + suffix + '_COMMAND: &str = ' + JSON.stringify(sql) + ';');
    }
    expect(rust).toContain('READ_STRIPE_PAYMENT_PROPERTY, READ_STAY_COMMAND, READ_CASHIER_COMMAND, READ_POS_COMMAND, READ_NIGHT_COMMAND, READ_HOTEL_SOURCES, READ_HOUSEKEEPING_COMMAND, READ_SECTION_RELATED_RECORDS];');
  });

  it('plans all four reads in Rust and runs them on the caller unit with the same binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    const fetchMock = vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = scopedKindSql[body.kind];
      expect(sql).toBeTruthy();
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    });
    vi.stubGlobal('fetch', fetchMock);
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'record-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const stay = await nativePmsReadAll<{ id: string }>(PMS_RD_STAY_COMMAND, ['p', 't', 'stay', 'g', 'a', 'r'], db as never);
    const cashier = await nativePmsReadAll<{ id: string }>(PMS_RD_CASHIER_COMMAND, ['p', 't', 'account_close', 's', 'u'], db as never);
    const pos = await nativePmsReadAll<{ id: string }>(PMS_RD_POS_COMMAND, ['p', 'stay-1', 'source-1'], db as never);
    const night = await nativePmsReadAll<{ id: string }>(PMS_RD_NIGHT_COMMAND, ['p', '2026-11-24', 'night-1'], db as never);
    for (const rows of [stay, cashier, pos, night]) expect(rows.results[0]?.id).toBe('record-1');
    expect(bodies.map(b => b.kind)).toEqual([
      PMS_RD_STAY_COMMAND, PMS_RD_CASHIER_COMMAND, PMS_RD_POS_COMMAND, PMS_RD_NIGHT_COMMAND,
    ]);
    expect(bodies.map(b => b.values)).toEqual([
      ['p', 't', 'stay', 'g', 'a', 'r'],
      ['p', 't', 'account_close', 's', 'u'],
      ['p', 'stay-1', 'source-1'],
      ['p', '2026-11-24', 'night-1'],
    ]);
    expect(bound).toEqual(bodies.map(b => b.values));
    expect(prepared).toEqual([stayCommandSql, cashierCommandSql, posCommandSql, nightCommandSql]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
    const [url, init] = fetchMock.mock.calls[0] as unknown as [URL, RequestInit];
    expect(String(url)).toContain('/api/v1/holdings/acme/pms/command-plan');
    expect(init.headers).toMatchObject({ 'content-type': 'application/json', accept: 'application/json', 'cache-control': 'no-cache' });
  });
});
