import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  PMS_PROPERTY_POINT_HEAD_SQL,
  pmsRecordPropertyPointHeadSql,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r36-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r36-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

const PROP = 'SELECT * FROM pms_properties';
const REC = {
  exists: 'SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id',
  ownerFact: 'SELECT p.company_id,p.branch_id,NULL::text department_id,r.version,r.kind\n       FROM pms_records r JOIN pms_properties p ON p.id=r.property_id',
  guestEvidence: `SELECT r.id,r.property_id,r.data_json,p.name,p.business_date,p.config_json::jsonb->>'currency' currency\n    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id`,
} as const;

// Byte oracles: exact SQL captured at HEAD before this dedupe. The SHA-256 values
// are the pre-change inner text of each literal; every recomposition must match.
const ORIGINALS = [
  {
    file: 'adapters/attachments-finance-facts.ts',
    full: 'EXISTS(SELECT 1 FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND p.company_id=? AND r.version=? AND r.kind=?)',
    sha256: '399682228057e221da2487b3fb7fc382cfd6ca705ee1c8a2654c184814dc223e',
    recomposed: `EXISTS(${pmsRecordPropertyPointHeadSql('exists')} WHERE r.id=? AND p.company_id=? AND r.version=? AND r.kind=?)`,
    routed: "pmsRecordPropertyPointHeadSql('exists')",
  },
  {
    file: 'adapters/attachments-finance-facts.ts',
    full: 'SELECT p.company_id,p.branch_id,NULL::text department_id,r.version,r.kind\n       FROM pms_records r JOIN pms_properties p ON p.id=r.property_id\n       WHERE r.id=? AND p.company_id=?',
    sha256: 'ef2e5cf2622f27d229d6b5fcb157d4371249ba671b7ce05175add8ba4af75513',
    recomposed: `${pmsRecordPropertyPointHeadSql('ownerFact')}\n       WHERE r.id=? AND p.company_id=?`,
    routed: "pmsRecordPropertyPointHeadSql('ownerFact')",
  },
  {
    file: 'adapters/guest-account-finance-evidence.ts',
    full: `SELECT r.id,r.property_id,r.data_json,p.name,p.business_date,p.config_json::jsonb->>'currency' currency\n    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE r.id=? AND r.kind='guest_account'\n    AND p.company_id=? AND (?::text IS NULL OR p.branch_id=?)`,
    sha256: '350374e3f921c85ce7a4a39e8397e5fd6aefc40f093990ea1c2f1ee122f2f1dd',
    recomposed: `${pmsRecordPropertyPointHeadSql('guestEvidence')} WHERE r.id=? AND r.kind='guest_account'\n    AND p.company_id=? AND (?::text IS NULL OR p.branch_id=?)`,
    routed: "pmsRecordPropertyPointHeadSql('guestEvidence')",
  },
  {
    file: 'adapters/pms.ts',
    full: 'SELECT * FROM pms_properties WHERE id=? AND company_id=?',
    sha256: '2a763fc5df6fcab976d7c135ef3fe103dc3aa8cecc2fff090e5febc0e6eda357',
    recomposed: `${PMS_PROPERTY_POINT_HEAD_SQL} WHERE id=? AND company_id=?`,
    routed: '${PMS_PROPERTY_POINT_HEAD_SQL} WHERE id=? AND company_id=?',
  },
  {
    file: 'adapters/pms.ts',
    full: 'SELECT * FROM pms_properties WHERE company_id=?${facts.actor.branchId ? " AND branch_id=?" : ""} ORDER BY name',
    sha256: '51752fe2ad64442b43c7a333a5ca14ddaeb8ffa88aac49f4471a5c5fbcf5e9c9',
    recomposed: PMS_PROPERTY_POINT_HEAD_SQL + ' WHERE company_id=?${facts.actor.branchId ? " AND branch_id=?" : ""} ORDER BY name',
    routed: '${PMS_PROPERTY_POINT_HEAD_SQL} WHERE company_id=?${facts.actor.branchId ? " AND branch_id=?" : ""} ORDER BY name',
  },
  {
    file: 'adapters/pms-booking.ts',
    full: 'SELECT * FROM pms_properties WHERE id=?${facts.lock ? " FOR UPDATE" : ""}',
    sha256: 'a33e68474188c1b0add67cdfd17513fc828024289b941ad976faa9f540a9e446',
    recomposed: PMS_PROPERTY_POINT_HEAD_SQL + ' WHERE id=?${facts.lock ? " FOR UPDATE" : ""}',
    routed: '${PMS_PROPERTY_POINT_HEAD_SQL} WHERE id=?${facts.lock ? " FOR UPDATE" : ""}',
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

describe('pms read-tail r36 point-read head dedupe', () => {
  it('declares each byte-identical head exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split('PMS_PROPERTY_POINT_HEAD_SQL').length - 1).toBe(1);
    expect(shared.split('pmsRecordPropertyPointHeadSql').length - 1).toBe(1);
  });

  it('generates byte-identical heads for all four closed exports', () => {
    expect(PMS_PROPERTY_POINT_HEAD_SQL).toBe(PROP);
    expect(pmsRecordPropertyPointHeadSql('exists')).toBe(REC.exists);
    expect(pmsRecordPropertyPointHeadSql('ownerFact')).toBe(REC.ownerFact);
    expect(pmsRecordPropertyPointHeadSql('guestEvidence')).toBe(REC.guestEvidence);
  });

  it('recomposes all six original SQL literals byte-for-byte (binds unchanged)', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(oracle.recomposed).toBe(oracle.full);
      // same number of bound parameters, in the same order, on both sides
      expect((oracle.recomposed.match(/\?/g) ?? []).length).toBe((oracle.full.match(/\?/g) ?? []).length);
    }
  });

  it('routes all six call sites through the shared exports and removes inline heads', async () => {
    for (const oracle of ORIGINALS) {
      const text = await src(oracle.file);
      expect(text).toContain(oracle.routed);
    }
    const facts = await src('adapters/attachments-finance-facts.ts');
    expect(facts.split('pmsRecordPropertyPointHeadSql(').length - 1).toBe(2);
    expect(facts).not.toContain(REC.exists);
    expect(facts).not.toContain(REC.ownerFact);
    const guest = await src('adapters/guest-account-finance-evidence.ts');
    expect(guest).not.toContain(REC.guestEvidence);
    const pms = await src('adapters/pms.ts');
    expect(pms.split(PROP).length - 1).toBe(0);
    const booking = await src('adapters/pms-booking.ts');
    expect(booking.split(PROP).length - 1).toBe(0);
  });

  it('keeps every conditional lock literal in its caller', async () => {
    expect(await src('adapters/pms-booking.ts')).toContain('${facts.lock ? " FOR UPDATE" : ""}');
    const pms = await src('adapters/pms.ts');
    expect(pms).toContain('FOR UPDATE OF p');
    expect(pms).toContain('${facts.actor.branchId ? " AND branch_id=?" : ""}');
    expect(await src('adapters/pms-stripe.ts')).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
  });

  it('leaves locks, r17-r35 fragments and the native read registry untouched', async () => {
    const shared = await src(SHARED);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 332 to 323', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(313);
    expect(bare).toBe(315);
    expect(spaced).toBeLessThan(332);
  });
});
