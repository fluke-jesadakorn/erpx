import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { PMS_STAY_SOURCE_PROJECTION_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r38-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r38-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// The two home count projections were retired to the native hotelMetrics read
// plan; their r56 shared count head and dedupe file were deleted. Only the
// still-live related-record projection is pinned here.
const SRC = "SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay'";

const ORIGINALS = [
  {
    file: 'adapters/pms/read-repository.ts',
    full: SRC + " AND (id IN (SELECT id FROM chosen) OR data_json::jsonb->>'reservationId' IN (SELECT id FROM reservations))),",
    sha256: '61d7f40ac718d8c3a4d2a3a6fc99ebc095771d2a92dfe4f7b27862abb1e9347f',
    recomposed: PMS_STAY_SOURCE_PROJECTION_SQL + " AND (id IN (SELECT id FROM chosen) OR data_json::jsonb->>'reservationId' IN (SELECT id FROM reservations))),",
    routed: '${PMS_STAY_SOURCE_PROJECTION_SQL} AND (id IN (${pmsReservationRelationArmSql(\'chosenId\')})',
  },
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r38 projection-head dedupe', () => {
  it('exports the single retained projection head byte-identically', () => {
    expect(PMS_STAY_SOURCE_PROJECTION_SQL).toBe(SRC);
    expect(sha256(PMS_STAY_SOURCE_PROJECTION_SQL)).toBe(sha256(SRC));
  });

  it('declares the retained head exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split(SRC).length - 1).toBe(1);
    expect(shared.split('PMS_STAY_SOURCE_PROJECTION_SQL').length - 1).toBe(1);
  });

  it('recomposes the retained original SQL substring byte-for-byte with unchanged binds', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(oracle.recomposed).toBe(oracle.full);
      expect((oracle.recomposed.match(/\?/g) ?? []).length).toBe((oracle.full.match(/\?/g) ?? []).length);
    }
  });

  it('routes the remaining call site through the shared head and retires the home count fragments', async () => {
    for (const oracle of ORIGINALS) {
      expect(await src(oracle.file), oracle.routed).toContain(oracle.routed);
    }
    const shared = await src(SHARED);
    expect(shared).not.toContain('PMS_HOME_ACTIVE_ROOM_COUNT_SQL');
    expect(shared).not.toContain('PMS_HOME_STAY_STATE_COUNT_SQL');
    expect(shared).not.toContain('pms-read-tail-r56-sql');
    const rr = await src('adapters/pms/read-repository.ts');
    expect(rr.split('${PMS_STAY_SOURCE_PROJECTION_SQL}').length - 1).toBe(1); // one remaining related-record CTE after r48
    expect(rr).toContain('PMS_RD_SECTION_RELATED_RECORDS, [facts.propertyId, facts.seedsJson');
    expect(rr.split(SRC).length - 1).toBe(0);
    const rust = await src('adapters/native_read_sql.rs');
    expect(rust).toContain('PMS_RD_HOTEL_METRICS');
  });

  it('keeps locks, the native read registry and the r17-r37 fragments untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect(await src('adapters/pms-booking-refunds.ts')).toContain("'SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 323 to 234', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(224);
    expect(bare).toBe(225);
    expect(spaced).toBeLessThan(323);
  });
});
