import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePmsReadAll,
  nativePmsReadFirst,
  PMS_RD_INVENTORY_COMMAND,
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
