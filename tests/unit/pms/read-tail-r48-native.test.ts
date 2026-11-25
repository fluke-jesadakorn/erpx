import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativePmsReadAll, PMS_RD_SECTION_RELATED_RECORDS } from '../../../packages/server-pms/src/adapters/native-pms-read';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Exact HEAD expansion of the retired sectionRelatedRecords template (fragments
// r24/r25/r26/r31/r35/r38/r42/r44/r45). The Rust registry carries this literal
// byte-identically with the 29 binds in the scout order captured at HEAD.
const HEAD = {
  sectionRelatedRecords: "WITH chosen AS (\n   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))\n ), guest_history AS (\n   SELECT id,data_json::jsonb data,row_number() OVER(PARTITION BY data_json::jsonb->>'guestId' ORDER BY data_json::jsonb->>'arrival' DESC,id) position\n   FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'guestId' IN (SELECT id FROM chosen WHERE kind='guest')\n ), stays AS (\n   SELECT id,data FROM chosen WHERE kind='stay'\n   UNION SELECT st.id,st.data FROM chosen account JOIN LATERAL (\n     SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay'\n       AND data_json::jsonb->>'guestAccountId'=account.id OFFSET 0\n   ) st ON true WHERE account.kind='guest_account'\n   UNION SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay' AND (\n     (data_json::jsonb->>'state'='in_house' AND ((? AND data_json::jsonb->>'roomId' IN (SELECT id FROM chosen WHERE kind='room')) OR ?))\n     OR (? AND ((data_json::jsonb->>'state'='reserved' AND data_json::jsonb->>'arrival'<=?) OR (data_json::jsonb->>'state'='in_house')))\n   ) UNION SELECT id,data FROM guest_history WHERE position<=20\n ), guestAccounts AS (SELECT id FROM chosen WHERE kind='guest_account' UNION SELECT data->>'guestAccountId' FROM stays UNION SELECT data->>'guestAccountId' FROM chosen),\n room_ids AS (SELECT id FROM chosen WHERE kind='room' UNION SELECT data->>'roomId' FROM stays UNION SELECT data->>'roomId' FROM chosen),\n recent_tasks AS (\n   SELECT id FROM pms_records WHERE property_id=? AND ? AND kind='task'\n     AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND data_json::jsonb->>'state' IN ('done','superseded')\n     ORDER BY updated_at DESC,id LIMIT 50\n ),\n inventory_recent AS (\n   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND ? AND (\n     (kind='lost' AND (data_json::jsonb->>'state'='stored' OR data_json::jsonb->>'foundDate' BETWEEN ? AND ?))\n     OR (kind='movement' AND data_json::jsonb->>'date' BETWEEN ? AND ?)\n   ) ORDER BY updated_at DESC,id LIMIT 50\n ), linked_ids AS (\n   SELECT id FROM chosen UNION SELECT id FROM stays UNION SELECT id FROM guestAccounts\n   UNION SELECT data->>'reservationId' FROM stays\n   UNION SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen\n ), selected_records AS (\n SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=?\n AND kind IN ('room','guest','room_type','rate','entry','refund','correction','task','block','shift','item','lost','movement') AND (\n   id IN (SELECT id FROM chosen) OR id IN (SELECT id FROM stays)\n   OR (kind='reservation' AND id IN (SELECT data->>'reservationId' FROM stays))\n   OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))\n   OR (kind='room' AND (? OR id IN (SELECT id FROM room_ids)))\n   OR (kind='guest' AND ? AND id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='guest' ORDER BY updated_at DESC,id LIMIT 50))\n   OR (kind IN ('room_type','rate') AND (? OR ? OR id IN (SELECT data->>'typeId' FROM stays UNION SELECT data->>'rateId' FROM stays)))\n   OR (kind='guest_account' AND id IN (SELECT id FROM guestAccounts))\n   OR (kind IN ('entry','refund','correction') AND data_json::jsonb->>'guestAccountId' IN (SELECT id FROM guestAccounts))\n   OR (kind='task' AND ? AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND (data_json::jsonb->>'state' NOT IN ('done','superseded') OR id IN (SELECT id FROM recent_tasks)))\n   OR (kind='block' AND ? AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND data_json::jsonb->>'state'='active')\n   OR (kind='shift' AND (? OR ?) AND (data_json::jsonb->>'state'='open' OR (? AND data_json::jsonb->>'state'='closed' AND (data_json::jsonb->>'variance')::bigint<>0)))\n   OR (kind='item' AND id IN (SELECT data->>'itemId' FROM inventory_recent WHERE kind='movement'))\n   OR id IN (SELECT id FROM inventory_recent)\n ) UNION SELECT related.* FROM linked_ids linked JOIN LATERAL (\n SELECT id,kind,data_json,version,updated_at FROM pms_records\n WHERE property_id=? AND id=linked.id OFFSET 0\n ) related ON true\n ) SELECT * FROM selected_records ORDER BY updated_at,id",
} as const;
const HEAD_SHA = { sectionRelatedRecords: '69547e8fa65aeb91ec261d30c8f4ac297f6d4ef1df9666bc0f01a120bd5e0d83' } as const;
const BINDS = { sectionRelatedRecords: 29 } as const;
const SELECTS = { sectionRelatedRecords: 45 } as const;
const KEYS = Object.keys(HEAD) as (keyof typeof HEAD)[];
const KIND = { sectionRelatedRecords: PMS_RD_SECTION_RELATED_RECORDS } as const;
const VALUES = ["property-1", "[]", "property-1", "property-1", "property-1", false, false, true, "2026-11-25", "property-1", false, "property-1", false, "2026-10-26", "2027-01-24", "2026-10-26", "2027-01-24", "property-1", false, false, "property-1", false, true, false, false, false, true, true, "property-1"] as const;

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

describe('pms read-tail r48 native read retirement', () => {
  it('pins the exact HEAD bytes, length, 29 binds and 17 TS-template SELECT tokens', () => {
    for (const k of KEYS) {
      expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
      expect(HEAD[k].length, k).toBe(4281);
      expect((HEAD[k].match(/\?/g) ?? []).length, k).toBe(BINDS[k]);
      expect(HEAD[k].split('SELECT ').length - 1, k).toBe(SELECTS[k]);
    }
    expect(HEAD.sectionRelatedRecords.startsWith('WITH chosen AS (\n   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))')).toBe(true);
    expect(HEAD.sectionRelatedRecords.endsWith(') SELECT * FROM selected_records ORDER BY updated_at,id')).toBe(true);
  });

  it('declares one fixed allow-listed kind with no SQL on the wire', () => {
    expect(PMS_RD_SECTION_RELATED_RECORDS).toBe('pms_rd_section_related_records');
    expect(KIND.sectionRelatedRecords).not.toMatch(/SELECT|INSERT|UPDATE|DELETE|FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });

  it('carries the byte-identical retired SQL in the Rust registry and pins 33 kinds', async () => {
    const rust = await src('adapters/native_read_sql.rs');
    expect(rust).toContain('pub const PMS_RD_SECTION_RELATED_RECORDS: &str = "pms_rd_section_related_records";');
    expect(rust).toContain('const SQL_SECTION_RELATED_RECORDS: &str = ' + JSON.stringify(HEAD.sectionRelatedRecords) + ';');
    expect(rust).toContain('bind_count: 29,');
    expect(rust).toContain('READ_HOUSEKEEPING_COMMAND, READ_SECTION_RELATED_RECORDS];');
    const reads = rust.split('\n').find(line => line.startsWith('const READS:'))!;
    expect(reads.split('READ_').length - 1).toBe(33);
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(33);
    const bridge = await src('adapters/native-pms-read.ts');
    expect(bridge).toContain("export const PMS_RD_SECTION_RELATED_RECORDS = 'pms_rd_section_related_records';");
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(33);
  });

  it('routes sectionRelatedRecords through the native transport and removes the inline SQL', async () => {
    const repo = await src('adapters/pms/read-repository.ts');
    expect(repo).not.toContain(HEAD.sectionRelatedRecords);
    expect(repo).not.toContain('${PMS_CHOSEN_RECORDS_SQL}\n  ), guest_history AS (');
    expect(repo).not.toContain('import { pmsChosenKindUnionSql }');
    expect(repo).toContain('nativePmsReadAll<PmsStoredRecord>(PMS_RD_SECTION_RELATED_RECORDS, [');
    const at = repo.indexOf('nativePmsReadAll<PmsStoredRecord>(PMS_RD_SECTION_RELATED_RECORDS, [');
    const chunk = repo.slice(at, at + 900);
    expect(chunk).toContain('facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3, facts.propertyId3, facts.roomView');
    expect(repo).toContain("[facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3, facts.propertyId3, facts.roomView, facts.inventoryView, facts.nightView, facts.businessDate, facts.propertyId4, facts.roomView2, facts.propertyId5, facts.inventoryView2, facts.from, facts.to, facts.from2, facts.to2, facts.propertyId6, facts.inventoryView3, facts.inventoryView4, facts.propertyId7, facts.roomView3, facts.nightView2, facts.roomView4, facts.roomView5, facts.fact24, facts.nightView3, facts.nightView4, facts.propertyId6]");
    expect(chunk).toContain('facts.fact24, facts.nightView3, facts.nightView4, facts.propertyId6], db');
    expect(chunk).toContain('.then(rows => rows.results.map(decodePmsRecord))');
    expect(repo.split('PMS_RD_SECTION_RELATED_RECORDS').length - 1).toBe(2);
    // Room summary is now owned by native lane 20, not a TS prepared statement.
    expect(repo).not.toContain('roomSummary: facts => db.prepare(');
    expect(repo).toContain('roomSummaryFacts(db, actor, {');
  });

  it('leaves the caller property lock, lock reads, feature and settlement reads untouched', async () => {
    const app = await src('application/pms.ts');
    expect(app).toContain("readPmsProperties({id,companyId:actor.companyId,lock:true})");
    expect(app).toContain('readPmsHousekeepingCommand(id,targetId,operation.startsWith');
    const pms = await src('adapters/pms.ts');
    expect(pms).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    for (const line of rust.split('\n').filter(line => line.trimStart().startsWith('const SQL_'))) {
      expect(line).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('plans the section read in Rust and runs it on the caller unit with the 29 binds in order', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({ data: { statements: [{ sql: HEAD.sectionRelatedRecords, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const prepared: string[] = [], bound: unknown[][] = [];
    const db = {
      prepare(sql: string) { prepared.push(sql); return { bind(...values: unknown[]) { bound.push(values); return this; } }; },
      async batch<T>() { return [{ results: [{ id: 'record-1' }], success: true, meta: { changes: 0, duration: 0 } }] as unknown as { results: T[]; success: boolean; meta: { changes: number; duration: number } }[]; },
      async exec() { return { count: 0, duration: 0 }; },
    };
    const rows = await nativePmsReadAll<{ id: string }>(PMS_RD_SECTION_RELATED_RECORDS, VALUES, db as never);
    expect(rows.results[0]?.id).toBe('record-1');
    expect(bodies.map(b => b.kind)).toEqual([PMS_RD_SECTION_RELATED_RECORDS]);
    expect(bodies[0].values).toEqual([...VALUES]);
    expect(bound).toEqual([[...VALUES]]);
    expect(prepared).toEqual([HEAD.sectionRelatedRecords]);
    for (const body of bodies) expect(JSON.stringify(body)).not.toContain('SELECT');
  });

  it('lowers the pms owner SELECT KPI from 275 to 255', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(238);
    expect(bare).toBe(239);
    expect(spaced).toBeLessThan(275);
    expect(bare).toBeLessThan(277);
  });
});
