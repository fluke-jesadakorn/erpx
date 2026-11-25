import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { PMS_RECORDS_COUNT_HEAD_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r56-sql';
import {
  PMS_HOME_ACTIVE_ROOM_COUNT_SQL,
  PMS_HOME_STAY_STATE_COUNT_SQL,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r38-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const count = (text: string, needle: string) => text.split(needle).length - 1;

// The byte-identical owner-local head retired at HEAD (KPI 243). Both home
// count projections opened with these exact bytes and diverged only in their
// closed alias/predicate tails.
const HEAD = 'SELECT count(*) FROM pms_records ';
const HEAD_SHA = '6ca860682e4b3afc3cbf25354cbc6a5957ab1243cdb319ff61ddceae265791df';
const TAILS = [
  {
    name: 'pms_home active-room count projection',
    tail: "r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'",
    sha: 'f35943aacea0a4cac80159bbd72da33c1aa394bc9ecb5ec7edbf0ce57bea7f02',
    selects: 1,
    binds: 0,
  },
  {
    name: 'pms_home stay-state count projection',
    tail: "s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'",
    sha: '2ed4fdc68472df4e524392d0525fca1da3dada81521ebe0c0adc9e18c6b6f32d',
    selects: 1,
    binds: 0,
  },
] as const;

const RENDERER = 'adapters/pms-read-tail-r56-sql.ts';
const SHARED = 'adapters/pms-read-tail-r38-sql.ts';
const PMS = 'adapters/pms.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r56 owner-local count-head dedupe', () => {
  it('pins the exact HEAD bytes of the shared home count head', () => {
    expect(sha256(HEAD), 'head sha').toBe(HEAD_SHA);
    expect(HEAD.length, 'head length').toBe(33);
    expect(count(HEAD, 'SELECT '), 'head selects').toBe(1);
    expect(count(HEAD, '?'), 'head binds').toBe(0);
    expect(PMS_RECORDS_COUNT_HEAD_SQL).toBe(HEAD);
    expect(sha256(PMS_RECORDS_COUNT_HEAD_SQL)).toBe(HEAD_SHA);
  });

  it('recomposes both count projections byte-identically with unchanged selects and binds', () => {
    const exports = [PMS_HOME_ACTIVE_ROOM_COUNT_SQL, PMS_HOME_STAY_STATE_COUNT_SQL];
    for (let i = 0; i < TAILS.length; i++) {
      const o = TAILS[i];
      const rendered = PMS_RECORDS_COUNT_HEAD_SQL + o.tail;
      expect(sha256(rendered), o.name).toBe(o.sha);
      expect(rendered.startsWith(HEAD), o.name).toBe(true);
      expect(count(rendered, 'SELECT '), o.name).toBe(o.selects);
      expect(count(rendered, '?'), o.name).toBe(o.binds);
      expect(exports[i], o.name).toBe(rendered);
    }
  });

  it('declares the head exactly once in the owner-local renderer with no write/lock seam', async () => {
    const shared = await src(RENDERER);
    expect(count(shared, HEAD)).toBe(1);
    expect(count(shared, 'SELECT ')).toBe(1);
    expect(count(shared, 'PMS_RECORDS_COUNT_HEAD_SQL')).toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('routes both sites through the renderer and retires the inline heads', async () => {
    const shared = await src(SHARED);
    expect(count(shared, 'PMS_RECORDS_COUNT_HEAD_SQL')).toBe(3); // import + 2 uses
    expect(shared).toContain("import { PMS_RECORDS_COUNT_HEAD_SQL } from './pms-read-tail-r56-sql';");
    expect(shared).toContain('${PMS_RECORDS_COUNT_HEAD_SQL}r WHERE r.property_id=p.id AND r.kind=\'room\'');
    expect(shared).toContain('${PMS_RECORDS_COUNT_HEAD_SQL}s WHERE s.property_id=p.id AND s.kind=\'stay\'');
    expect(count(shared, HEAD), 'inline head retired').toBe(0);
  });

  it('leaves locks, native registry, cross-owner families and prior helpers untouched', async () => {
    expect(await src(PMS)).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(33);
    expect(rust).toContain('const SQL_STAY_COMMAND: &str = "WITH scope AS (');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(33);
    // The pms_properties family has cross-owner consumers and stays inline.
    expect(await src('adapters/pms-read-tail-r20-sql.ts')).toContain('export const PMS_PROPERTIES_PROJECTION_SQL = "SELECT');
    expect(await src('adapters/pms-read-tail-r29-sql.ts')).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 49, 50, 51, 53, 55, 56]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 243 to 242', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += count(text, 'SELECT ');
      bare += count(text, 'SELECT');
    }
    expect(spaced).toBe(239);
    expect(bare).toBe(240);
    expect(spaced).toBeLessThan(243);
    expect(bare).toBeLessThan(245);
  });
});
