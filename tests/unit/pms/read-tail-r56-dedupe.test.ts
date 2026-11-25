import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const exists = async (name: string) => {
  try { await src(name); return true; } catch { return false; }
};

// r56 was the owner-local renderer for the two home count heads. The
// hotelMetrics native cut moved those exact bytes into the allow-listed Rust
// plan, removed the fragments from r38 and deleted the renderer file. This
// suite is now the retirement check that no TypeScript copy survives.
const RETIRED_SOURCE = 'adapters/pms-read-tail-r56-sql.ts';
const RETIRED_FRAGMENTS = ['PMS_HOME_ACTIVE_ROOM_COUNT_SQL', 'PMS_HOME_STAY_STATE_COUNT_SQL'] as const;
const RETIRED_BYTES = [
  "SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'",
  "SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'",
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r56 retirement after the hotelMetrics native cut', () => {
  it('deletes the renderer file and both dead count fragments', async () => {
    expect(await exists(RETIRED_SOURCE)).toBe(false);
    const r38 = await src('adapters/pms-read-tail-r38-sql.ts');
    for (const fragment of RETIRED_FRAGMENTS) expect(r38).not.toContain(fragment);
    const r41 = await src('adapters/pms-read-tail-r41-sql.ts');
    for (const fragment of RETIRED_FRAGMENTS) expect(r41).not.toContain(fragment);
    for (const ts of await tsFiles(new URL('adapters/', dir))) {
      const text = await readFile(ts, 'utf8');
      for (const fragment of RETIRED_FRAGMENTS) expect(text).not.toContain(fragment);
    }
  });

  it('keeps the retired count bytes only in the native hotelMetrics plan', async () => {
    const rust = await src('adapters/native_read_sql.rs');
    expect(rust).toContain('pub const PMS_RD_HOTEL_METRICS: &str = "pms_rd_hotel_metrics";');
    for (const bytes of RETIRED_BYTES) expect(rust).toContain(bytes);
    const bridge = await src('adapters/native-pms-read.ts');
    expect(bridge).toContain("export const PMS_RD_HOTEL_METRICS = 'pms_rd_hotel_metrics';");
    const home = await src('adapters/pms/home.ts');
    expect(home).toContain('PMS_RD_HOTEL_METRICS, [facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2], db');
  });

  it('leaves locks, the native registry, cross-owner families and prior helpers untouched', async () => {
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(61);
    expect(rust).toContain('const SQL_STAY_COMMAND: &str = "WITH scope AS (');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(61);
    // The pms_properties family has cross-owner consumers and stays inline.
    expect(await src('adapters/pms-read-tail-r20-sql.ts')).toContain('export const PMS_PROPERTIES_PROJECTION_SQL = "SELECT');
    expect(await src('adapters/pms-read-tail-r29-sql.ts')).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 49, 50, 51, 53, 55]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 243 to 234', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(207);
    expect(bare).toBe(208);
    expect(spaced).toBeLessThan(243);
    expect(bare).toBeLessThan(245);
  });
});
