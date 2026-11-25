import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { PMS_RESERVATION_ID_ARM_HEAD_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r57-sql';
import { pmsReservationRelationArmSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r45-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const count = (text: string, needle: string) => text.split(needle).length - 1;

// Exact bytes captured from live HEAD d107e637 before the fold. Both relation
// arms opened with this identical head and diverged only in their closed source
// tail; binds, UNION order and result types stay at the call site.
const HEAD = "SELECT data->>'reservationId' ";
const HEAD_SHA = 'c58a644705b20ee81ddeedc191434933e81111a1ac5f47f1e6a7b0efff7f77ff';
const ARMS = [
  {
    name: 'reservationIdChosen',
    tail: 'FROM chosen',
    tailSha: 'c1c548d6bb9a36b603c5e5b92b3f2a1905c13c9187b6ae2a771650afbabd2c4b',
    sha: '1e9a33b048f8f512b3f911aed0df97a6faf8c8a1cce6111e9de1944185b945a1',
    len: 41,
    selects: 1,
    binds: 0,
  },
  {
    name: 'reservationIdStays',
    tail: 'FROM stays',
    tailSha: '270edefa36d9653e73c5ed09b42cbaeba2b781f940ada91003c2b57ee8a098a9',
    sha: 'c2456ef7a1fc6ff5f7615c193cfc1284a4eda1da9155a4d9a2cd67d0a0c92a53',
    len: 40,
    selects: 1,
    binds: 0,
  },
] as const;

const RENDERER = 'adapters/pms-read-tail-r57-sql.ts';
const SHARED = 'adapters/pms-read-tail-r45-sql.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r57 reservation-id arm head dedupe', () => {
  it('pins the exact HEAD bytes of the shared reservation-id arm head', () => {
    expect(sha256(HEAD), 'head sha').toBe(HEAD_SHA);
    expect(HEAD.length, 'head length').toBe(30);
    expect(count(HEAD, 'SELECT '), 'head selects').toBe(1);
    expect(count(HEAD, '?'), 'head binds').toBe(0);
    expect(PMS_RESERVATION_ID_ARM_HEAD_SQL).toBe(HEAD);
    expect(sha256(PMS_RESERVATION_ID_ARM_HEAD_SQL)).toBe(HEAD_SHA);
  });

  it('recomposes both arms byte-identically with unchanged selects and binds', () => {
    for (const arm of ARMS) {
      expect(sha256(arm.tail), `${arm.name} tail sha`).toBe(arm.tailSha);
      const rendered = PMS_RESERVATION_ID_ARM_HEAD_SQL + arm.tail;
      expect(sha256(rendered), arm.name).toBe(arm.sha);
      expect(rendered.length, `${arm.name} length`).toBe(arm.len);
      expect(rendered.startsWith(HEAD), arm.name).toBe(true);
      expect(count(rendered, 'SELECT '), arm.name).toBe(arm.selects);
      expect(count(rendered, '?'), arm.name).toBe(arm.binds);
      expect(pmsReservationRelationArmSql(arm.name), arm.name).toBe(rendered);
    }
  });

  it('declares the head exactly once in the owner-local renderer with no write/lock seam', async () => {
    const renderer = await src(RENDERER);
    expect(count(renderer, HEAD)).toBe(1);
    expect(count(renderer, 'SELECT ')).toBe(1);
    expect(count(renderer, 'PMS_RESERVATION_ID_ARM_HEAD_SQL')).toBe(1);
    expect(renderer).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('routes both r45 arms through the renderer and retires the inline heads', async () => {
    const shared = await src(SHARED);
    expect(count(shared, 'PMS_RESERVATION_ID_ARM_HEAD_SQL')).toBe(3); // import + 2 uses
    expect(shared).toContain("import { PMS_RESERVATION_ID_ARM_HEAD_SQL } from './pms-read-tail-r57-sql';");
    expect(shared).toContain('reservationIdChosen: `${PMS_RESERVATION_ID_ARM_HEAD_SQL}FROM chosen`');
    expect(shared).toContain('reservationIdStays: `${PMS_RESERVATION_ID_ARM_HEAD_SQL}FROM stays`');
    expect(count(shared, HEAD), 'inline head retired').toBe(0);
    expect(await src('adapters/pms/read-repository.ts')).toContain("reservations AS (${pmsReservationRelationArmSql('chosenReservationId')} UNION ${pmsReservationRelationArmSql('reservationIdChosen')})");
  });

  it('leaves locks, native registry, cross-owner properties and sibling families untouched', async () => {
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(39);
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(39);
    // The pms_properties family has cross-owner consumers and stays inline.
    expect(await src('adapters/pms-read-tail-r20-sql.ts')).toContain('export const PMS_PROPERTIES_PROJECTION_SQL = "SELECT');
    expect(await src('adapters/pms-read-tail-r29-sql.ts')).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT');
    // The guest-id sibling family stays inline for a separate future slice.
    expect(await src('adapters/pms-read-tail-r25-sql.ts')).toContain('SELECT data->>\'guestId\' FROM stays UNION SELECT data->>\'guestId\' FROM chosen');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 49, 50, 51, 53, 55]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 242 to 241', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += count(text, 'SELECT ');
      bare += count(text, 'SELECT');
    }
    expect(spaced).toBe(229);
    expect(bare).toBe(230);
    expect(spaced).toBeLessThan(242);
    expect(bare).toBeLessThan(244);
  });
});
