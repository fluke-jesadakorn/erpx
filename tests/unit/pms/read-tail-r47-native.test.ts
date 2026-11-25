import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePmsReadAll,
  PMS_RD_HOTEL_SOURCES,
  PMS_RD_HOUSEKEEPING_COMMAND,
} from '../../../packages/server-pms/src/adapters/native-pms-read';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: the exact HEAD SQL captured before this retirement. The
// hotelSources recursive CTE keeps its single-space line indentation; the
// housekeeping CTE keeps its ten/eight-space indentation. Both are byte-expanded
// into the Rust registry verbatim, with binds and order untouched.
const HEAD = {
  hotelSources: "WITH RECURSIVE roots AS (\n SELECT r.id FROM pms_records r WHERE r.property_id=? AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey' LIKE 'night:%'\n UNION ALL SELECT r.id FROM pms_records r JOIN roots e ON r.data_json::jsonb->>'originalId'=e.id WHERE r.property_id=? AND r.kind='entry' AND r.id<>e.id\n ), sources AS (\n SELECT r.* FROM pms_records r WHERE r.property_id=? AND (\n (? IN ('occupancy','revpar') AND r.kind='room' AND r.data_json::jsonb->>'active'='true' AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=r.property_id AND b.kind='block' AND b.data_json::jsonb->>'state'='active' AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'from'<=? AND b.data_json::jsonb->>'roomId'=r.id))\n OR (? IN ('occupancy','adr') AND r.kind='stay' AND r.data_json::jsonb->>'state' IN ('in_house','checked_out') AND r.data_json::jsonb->>'arrival'<=? AND r.data_json::jsonb->>'departure'>?)\n OR (?<>'occupancy' AND r.kind='entry' AND r.id IN (SELECT id FROM roots) AND r.data_json::jsonb->>'date'=? AND r.data_json::jsonb->>'kind' IN ('charge','adjustment'))\n )) SELECT id,kind,version,data_json FROM sources ORDER BY kind,id LIMIT 31 OFFSET ?",
  housekeeping: "WITH target AS (\n          SELECT CASE WHEN kind='task' THEN data_json::jsonb->>'roomId' ELSE id END room_id FROM pms_records WHERE property_id=? AND kind=? AND id=?\n        ) SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r JOIN target t ON\n          (r.kind='room' AND r.id=t.room_id) OR (r.kind='task' AND r.data_json::jsonb->>'roomId'=t.room_id)\n          OR (r.kind='stay' AND r.data_json::jsonb->>'roomId'=t.room_id AND r.data_json::jsonb->>'state'='in_house')\n          WHERE r.property_id=? ORDER BY r.id",
} as const;
const HEAD_SHA = {
  hotelSources: '5dc886551ec3b63a8fd60ac77c75f2c6b8aff78438650af8bf06acf3e14bc2fe',
  housekeeping: 'd36eeefa6d74a55f4f66aaae6b62f1121081ffccf205f5712fab89abb5b712e6',
} as const;
const BINDS = { hotelSources: 11, housekeeping: 4 } as const;
const SELECTS = { hotelSources: 6, housekeeping: 2 } as const;
const KEYS = Object.keys(HEAD) as (keyof typeof HEAD)[];
const KIND = {
  hotelSources: PMS_RD_HOTEL_SOURCES,
  housekeeping: PMS_RD_HOUSEKEEPING_COMMAND,
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
const envKey = 'ERPX_NATIVE_PMS_CONFIG';
const originalEnv = process.env[envKey];

beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44995',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
  });
});
afterEach(() => {
  if (originalEnv === undefined) delete process.env[envKey];
  else process.env[envKey] = originalEnv;
  vi.unstubAllGlobals();
});

describe('pms read-tail r47 native read retirement', () => {
  it('pins the exact HEAD bytes, bind counts and SELECT counts of both oracles', () => {
    for (const k of KEYS) {
      expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
      expect((HEAD[k].match(/\?/g) ?? []).length, k).toBe(BINDS[k]);
      expect(HEAD[k].split('SELECT ').length - 1, k).toBe(SELECTS[k]);
    }
    expect(HEAD.hotelSources.startsWith('WITH RECURSIVE roots AS (')).toBe(true);
    expect(HEAD.hotelSources).toContain('UNION ALL SELECT r.id FROM pms_records r JOIN roots e');
    expect(HEAD.housekeeping.startsWith('WITH target AS (')).toBe(true);
    expect(HEAD.housekeeping.endsWith('WHERE r.property_id=? ORDER BY r.id')).toBe(true);
  });

  it('declares two fixed allow-listed kinds with no SQL on the wire', () => {
    expect(PMS_RD_HOTEL_SOURCES).toBe('pms_rd_hotel_sources');
    expect(PMS_RD_HOUSEKEEPING_COMMAND).toBe('pms_rd_housekeeping_command');
    for (const kind of Object.values(KIND)) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
    }
  });

  it('carries byte-identical retired SQL and both kinds in the Rust registry', async () => {
    const rust = await src('adapters/native_read_sql.rs');
    expect(rust).toContain('pub const PMS_RD_HOTEL_SOURCES: &str = "pms_rd_hotel_sources";');
    expect(rust).toContain('pub const PMS_RD_HOUSEKEEPING_COMMAND: &str = "pms_rd_housekeeping_command";');
    expect(rust).toContain('const SQL_HOTEL_SOURCES: &str = ' + JSON.stringify(HEAD.hotelSources) + ';');
    expect(rust).toContain('const SQL_HOUSEKEEPING_COMMAND: &str = ' + JSON.stringify(HEAD.housekeeping) + ';');
    expect(rust).toContain('bind_count: 11,');
    expect(rust).toContain('bind_count: 4,');
    expect(rust).toContain('READ_STAY_COMMAND, READ_CASHIER_COMMAND, READ_POS_COMMAND, READ_NIGHT_COMMAND, READ_HOTEL_SOURCES, READ_HOUSEKEEPING_COMMAND, READ_SECTION_RELATED_RECORDS];');
  });

  it('routes both call sites through the native transport and removes the inline SQL', async () => {
    const home = await src('adapters/pms/home.ts');
    expect(home).not.toContain(HEAD.hotelSources);
    expect(home).toContain('nativePmsReadAll');
    expect(home).toContain('PMS_RD_HOTEL_SOURCES');
    const hotelAt = home.indexOf('PMS_RD_HOTEL_SOURCES,');
    expect(hotelAt).toBeGreaterThanOrEqual(0);
    const hotelChunk = home.slice(Math.max(0, hotelAt - 400), hotelAt + 400);
    expect(hotelChunk).toContain('nativePmsReadAll');
    expect(hotelChunk).toContain('facts.offset], db');
    expect(home.split('nativePmsReadAll<').length - 1).toBe(1);

    const pms = await src('adapters/pms.ts');
    expect(pms).not.toContain(HEAD.housekeeping);
    expect(pms).toContain('readPmsHousekeepingCommand');
    expect(pms).toContain('PMS_RD_HOUSEKEEPING_COMMAND');
    const houseAt = pms.indexOf('PMS_RD_HOUSEKEEPING_COMMAND,');
    expect(houseAt).toBeGreaterThanOrEqual(0);
    const houseChunk = pms.slice(Math.max(0, houseAt - 200), houseAt + 200);
    expect(houseChunk).toContain('nativePmsReadAll');
    expect(houseChunk).toContain('[propertyId,targetKind,targetId,propertyId], db');
    expect(pms.split('PMS_RD_HOUSEKEEPING_COMMAND').length - 1).toBe(2);
  });

  it('leaves the caller property lock, lock reads, feature and settlement reads untouched', async () => {
    // The housekeeping caller unit still takes the property FOR UPDATE lock and
    // then executes the native read on the same db unit.
    const app = await src('application/pms.ts');
    expect(app).toContain("readPmsProperties({id,companyId:actor.companyId,lock:true})");
    expect(app).toContain('readPmsHousekeepingCommand(id,targetId,operation.startsWith');
    const pms = await src('adapters/pms.ts');
    expect(pms).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    // Only the planned SQL literals matter; comments and ledger-free assertions
    // may legitimately mention the lock clauses that stay in the caller's TS unit.
    for (const line of rust.split('\n').filter(line => line.trimStart().startsWith('const SQL_'))) {
      expect(line).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('plans both reads in Rust and runs them on the caller unit with the same binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === PMS_RD_HOTEL_SOURCES ? HEAD.hotelSources : HEAD.housekeeping;
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'record-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const hotelValues = ['property-1', 'property-1', 'property-1', 'occupancy', '2026-11-24', 'adr', '2026-11-23', '2026-11-25', 'revpar', '2026-11-26', 30];
    const houseValues = ['property-1', 'task', 'task-1', 'property-1'];
    const hotel = await nativePmsReadAll<{ id: string }>(PMS_RD_HOTEL_SOURCES, hotelValues, db as never);
    const house = await nativePmsReadAll<{ id: string }>(PMS_RD_HOUSEKEEPING_COMMAND, houseValues, db as never);

    expect(hotel.results[0]?.id).toBe('record-1');
    expect(house.results[0]?.id).toBe('record-1');
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_HOTEL_SOURCES, PMS_RD_HOUSEKEEPING_COMMAND]);
    expect(bodies.map(b => b.values)).toEqual([hotelValues, houseValues]);
    expect(bound).toEqual([hotelValues, houseValues]);
    expect(prepared).toEqual([HEAD.hotelSources, HEAD.housekeeping]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });

  it('lowers the pms owner SELECT KPI from 283 to 255', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(241);
    expect(bare).toBe(243);
    expect(spaced).toBeLessThan(283);
    expect(bare).toBeLessThan(285);
  });
});
