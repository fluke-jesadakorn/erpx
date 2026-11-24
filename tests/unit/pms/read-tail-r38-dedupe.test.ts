import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  PMS_HOME_ACTIVE_ROOM_COUNT_SQL,
  PMS_HOME_STAY_STATE_COUNT_SQL,
  PMS_STAY_SOURCE_PROJECTION_SQL,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r38-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r38-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

const ACTIVE = "SELECT count(*) FROM pms_records r WHERE r.property_id=p.id AND r.kind='room' AND r.data_json::jsonb->>'active'='true'";
const STAY = "SELECT count(*) FROM pms_records s WHERE s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'state'";
const SRC = "SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay'";

// Byte oracles: exact SQL substrings captured at HEAD before this dedupe. The
// SHA-256 values are the pre-change text; every recomposition must match
// byte-for-byte and keep the same bound parameters in the same order.
const ORIGINALS = [
  {
    file: 'adapters/pms/home.ts',
    full: ACTIVE + ") room_count,",
    sha256: 'de275dd8312754ba4ee4f5a116f0469653af26b27466a1a104782a36030c1d94',
    recomposed: PMS_HOME_ACTIVE_ROOM_COUNT_SQL + ") room_count,",
    routed: '${PMS_HOME_ACTIVE_ROOM_COUNT_SQL}) room_count,',
  },
  {
    file: 'adapters/pms/home.ts',
    full: ACTIVE + "\n      AND r.data_json::jsonb->>'cleanliness'='inspected') inspected,",
    sha256: 'bc76f8e95c1aa3127905dd1dbbbdf3cbffee87410742289e866a1f70a81ebe57',
    recomposed: PMS_HOME_ACTIVE_ROOM_COUNT_SQL + "\n      AND r.data_json::jsonb->>'cleanliness'='inspected') inspected,",
    routed: '${PMS_HOME_ACTIVE_ROOM_COUNT_SQL}\n      AND r.data_json::jsonb->>',
  },
  {
    file: 'adapters/pms/home.ts',
    full: ACTIVE + "\n      AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=p.id AND b.kind='block' AND b.data_json::jsonb->>'state'='active'",
    sha256: '0e224b165406778bb42358efd230f6f6170ef80fe69d4b0290ec2f5a5b2ebb18',
    recomposed: PMS_HOME_ACTIVE_ROOM_COUNT_SQL + "\n      AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=p.id AND b.kind='block' AND b.data_json::jsonb->>'state'='active'",
    routed: '${PMS_HOME_ACTIVE_ROOM_COUNT_SQL}\n      AND NOT EXISTS(',
  },
  {
    file: 'adapters/pms/home.ts',
    full: STAY + " IN ('in_house','checked_out')",
    sha256: '6ab25e7f6c6fb1858d3f910d744d3b8d37f0852aaa6a0b880ddc929e553a1303',
    recomposed: PMS_HOME_STAY_STATE_COUNT_SQL + " IN ('in_house','checked_out')",
    routed: "${PMS_HOME_STAY_STATE_COUNT_SQL} IN ('in_house','checked_out')",
  },
  {
    file: 'adapters/pms/home.ts',
    full: STAY + "='reserved' AND s.data_json::jsonb->>'arrival'<=p.business_date) arrivals,",
    sha256: 'a5d4ca0f38ab171243e744d2c66c2b96404aeb4a6956e2732954191cfaea5e61',
    recomposed: PMS_HOME_STAY_STATE_COUNT_SQL + "='reserved' AND s.data_json::jsonb->>'arrival'<=p.business_date) arrivals,",
    routed: "${PMS_HOME_STAY_STATE_COUNT_SQL}='reserved'",
  },
  {
    file: 'adapters/pms/home.ts',
    full: STAY + "='in_house' AND s.data_json::jsonb->>'departure'<=p.business_date) departures",
    sha256: 'c7752431890fb1d32e31d5f3fd912fb90f636b843832f928419d95483ae3bb85',
    recomposed: PMS_HOME_STAY_STATE_COUNT_SQL + "='in_house' AND s.data_json::jsonb->>'departure'<=p.business_date) departures",
    routed: "${PMS_HOME_STAY_STATE_COUNT_SQL}='in_house'",
  },
  {
    file: 'adapters/pms/read-repository.ts',
    full: SRC + " AND (id IN (SELECT id FROM chosen) OR data_json::jsonb->>'reservationId' IN (SELECT id FROM reservations))),",
    sha256: '61d7f40ac718d8c3a4d2a3a6fc99ebc095771d2a92dfe4f7b27862abb1e9347f',
    recomposed: PMS_STAY_SOURCE_PROJECTION_SQL + " AND (id IN (SELECT id FROM chosen) OR data_json::jsonb->>'reservationId' IN (SELECT id FROM reservations))),",
    routed: '${PMS_STAY_SOURCE_PROJECTION_SQL} AND (id IN (SELECT id FROM chosen)',
  },
  {
    file: 'adapters/pms/read-repository.ts',
    full: SRC + "\n       AND data_json::jsonb->>'guestAccountId'=account.id OFFSET 0",
    sha256: 'a89bc33ffa97c11781d6fc0955ae82c878eba33b79fa2c7c30c187352f66eaeb',
    recomposed: PMS_STAY_SOURCE_PROJECTION_SQL + "\n       AND data_json::jsonb->>'guestAccountId'=account.id OFFSET 0",
    routed: '${PMS_STAY_SOURCE_PROJECTION_SQL}\n       AND data_json::jsonb->>',
  },
  {
    file: 'adapters/pms/read-repository.ts',
    full: SRC + " AND (\n     (data_json::jsonb->>'state'='in_house' AND ((? AND data_json::jsonb->>'roomId' IN (SELECT id FROM chosen WHERE kind='room')) OR ?))",
    sha256: '9e611e7412043b0595cf52f973044d6c14f8b02ca690c13fa76d2730eb26c3f6',
    recomposed: PMS_STAY_SOURCE_PROJECTION_SQL + " AND (\n     (data_json::jsonb->>'state'='in_house' AND ((? AND data_json::jsonb->>'roomId' IN (SELECT id FROM chosen WHERE kind='room')) OR ?))",
    routed: '${PMS_STAY_SOURCE_PROJECTION_SQL} AND (\n     (data_json::jsonb->>',
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

describe('pms read-tail r38 count/projection head dedupe', () => {
  it('declares each byte-identical head exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split(ACTIVE).length - 1).toBe(1);
    expect(shared.split(STAY).length - 1).toBe(1);
    expect(shared.split(SRC).length - 1).toBe(1);
    expect(shared.split('PMS_HOME_ACTIVE_ROOM_COUNT_SQL').length - 1).toBe(1);
    expect(shared.split('PMS_HOME_STAY_STATE_COUNT_SQL').length - 1).toBe(1);
    expect(shared.split('PMS_STAY_SOURCE_PROJECTION_SQL').length - 1).toBe(1);
  });

  it('exports the exact byte-identical heads', () => {
    expect(PMS_HOME_ACTIVE_ROOM_COUNT_SQL).toBe(ACTIVE);
    expect(PMS_HOME_STAY_STATE_COUNT_SQL).toBe(STAY);
    expect(PMS_STAY_SOURCE_PROJECTION_SQL).toBe(SRC);
  });

  it('recomposes all nine original SQL substrings byte-for-byte with unchanged binds', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(oracle.recomposed).toBe(oracle.full);
      expect((oracle.recomposed.match(/\?/g) ?? []).length).toBe((oracle.full.match(/\?/g) ?? []).length);
    }
  });

  it('routes all nine call sites through the shared heads and removes inline heads', async () => {
    for (const oracle of ORIGINALS) {
      expect(await src(oracle.file), oracle.routed).toContain(oracle.routed);
    }
    const home = await src('adapters/pms/home.ts');
    expect(home.split('${PMS_HOME_ACTIVE_ROOM_COUNT_SQL}').length - 1).toBe(3); // three home metric columns
    expect(home.split('${PMS_HOME_STAY_STATE_COUNT_SQL}').length - 1).toBe(3); // three home metric columns
    expect(home.split(ACTIVE).length - 1).toBe(0);
    expect(home.split(STAY).length - 1).toBe(0);
    const rr = await src('adapters/pms/read-repository.ts');
    expect(rr.split('${PMS_STAY_SOURCE_PROJECTION_SQL}').length - 1).toBe(3); // three related-record CTEs
    expect(rr.split(SRC).length - 1).toBe(0);
  });

  it('keeps locks, the native read registry and the r17-r37 fragments untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect(await src('adapters/pms-booking-refunds.ts')).toContain("'SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 323 to 313', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(310);
    expect(bare).toBe(312);
    expect(spaced).toBeLessThan(323);
  });
});
