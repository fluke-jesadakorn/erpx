import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import {
  PMS_RECORD_ID_PROPERTY_HEAD_SQL,
  pmsSourceRecipientProbeSql,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r35-sql';
import { pmsSourcesSql } from '../../../packages/server-pms/src/adapters/pms/source-policy';
import { PMS_RECIPIENT_CROSS_JOIN_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r21-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r35-sql.ts';

const HEAD = 'SELECT id FROM pms_records WHERE property_id=?';
const PROBE_LITERAL = 'SELECT 1 FROM (${pmsSourcesSql}) pr ${PMS_RECIPIENT_CROSS_JOIN_SQL}';
const PROBE = `SELECT 1 FROM (${pmsSourcesSql}) pr ${PMS_RECIPIENT_CROSS_JOIN_SQL}`;

const ORIGINAL_HEADS = [
  'SELECT id FROM pms_records WHERE property_id=? AND kind=?',
  "SELECT id FROM pms_records WHERE property_id=? AND kind='block'",
  "SELECT id FROM pms_records WHERE property_id=? AND ? AND kind='task'",
  "SELECT id FROM pms_records WHERE property_id=? AND kind='guest' ORDER BY updated_at DESC,id LIMIT 50",
  'SELECT id FROM pms_records WHERE property_id=? AND kind=? ${eligible}',
];
const TAILS = [
  ' AND kind=?',
  " AND kind='block'",
  " AND kind='guest' ORDER BY updated_at DESC,id LIMIT 50",
  ' AND kind=? ${eligible}',
];
const PROBE_FILES = [
  'adapters/operations-handover-facts.ts',
  'adapters/collaboration-home-sources.ts',
];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r35 point-read head dedupe', () => {
  it('declares each byte-identical head exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split(HEAD).length - 1).toBe(1);
    expect(shared.split(PROBE_LITERAL).length - 1).toBe(1);
  });

  it('generates byte-identical SQL from both closed exports', () => {
    expect(PMS_RECORD_ID_PROPERTY_HEAD_SQL).toBe(HEAD);
    expect(pmsSourceRecipientProbeSql()).toBe(PROBE);
  });

  it('recomposes the original five point-read heads byte-identically', () => {
    for (let i = 0; i < ORIGINAL_HEADS.length; i++) {
      expect(PMS_RECORD_ID_PROPERTY_HEAD_SQL + TAILS[i]).toBe(ORIGINAL_HEADS[i]);
    }
  });

  it('routes the remaining read-repository heads through the constant', async () => {
    const text = await src('adapters/pms/read-repository.ts');
    expect(text.split(HEAD).length - 1).toBe(0);
    expect(text.split('PMS_RECORD_ID_PROPERTY_HEAD_SQL').length - 1).toBe(4); // import + 3 uses after r48
    for (const tail of TAILS) expect(text).toContain('${PMS_RECORD_ID_PROPERTY_HEAD_SQL}' + tail);
  });

  it('routes both source-recipient probes through the helper', async () => {
    for (const file of PROBE_FILES) {
      const text = await src(file);
      expect(text.split(PROBE_LITERAL).length - 1).toBe(0);
      expect(text.split('pmsSourceRecipientProbeSql()').length - 1).toBe(1);
    }
  });

  it('leaves locks, r17-r34 fragments and the native read registry untouched', async () => {
    const shared = await src(SHARED);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src('adapters/pms-stripe.ts')).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(await src('adapters/pms-booking-refunds.ts')).toContain("'SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32, 33, 34]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI strictly below the r34 after=337 baseline', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(258);
    expect(bare).toBe(260);
    expect(spaced).toBeLessThan(337);
  });
});
