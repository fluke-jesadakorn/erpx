import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r25-sql.ts';

const FRAGMENTS = [
  { constant: 'PMS_GUEST_ACCOUNT_ID_MEMBER_SQL', sql: "AND r.data_json::jsonb->>'guestAccountId' IN (SELECT id FROM account_ids)" },
  { constant: 'PMS_GUEST_ACCOUNT_CASHIER_MEMBER_SQL', sql: "WHERE r.property_id=q.property_id AND r.kind='guest_account' AND r.id IN (SELECT id FROM account_ids)" },
  { constant: 'PMS_RESERVATION_ID_MEMBER_SQL', sql: "AND r.data_json::jsonb->>'reservationId' IN (SELECT id FROM booking_ids)" },
  { constant: 'PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL', sql: "OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))" },
];

const PMS = 'adapters/pms.ts';
const REPO = 'adapters/pms/read-repository.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r25 byte-identical scoped member predicate dedupe', () => {
  it('declares each shared SQL text exactly once', async () => {
    const shared = await src(SHARED);
    for (const { sql } of FRAGMENTS) expect(shared.split(sql).length - 1).toBe(1);
  });

  it('routes every call site through the shared fragments with no inline literal', async () => {
    const pms = await src(PMS);
    const repo = await src(REPO);
    // import + uses
    expect(pms.split('PMS_GUEST_ACCOUNT_ID_MEMBER_SQL').length - 1).toBe(0); // r28 native port removed all uses and the import
    expect(pms.split('PMS_GUEST_ACCOUNT_CASHIER_MEMBER_SQL').length - 1).toBe(0); // r28 native port removed all uses and the import
    expect(pms.split('PMS_RESERVATION_ID_MEMBER_SQL').length - 1).toBe(0); // r28 native port removed all uses and the import
    expect(repo.split('PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL').length - 1).toBe(2); // import + 1 use after r48
    for (const { sql } of FRAGMENTS) {
      expect(pms).not.toContain(sql);
      expect(repo).not.toContain(sql);
    }
  });

  it('keeps the fragments read-only, leaves locks inline and does not re-dedupe r17-r24', async () => {
    for (const { sql } of FRAGMENTS) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    expect(await src(PMS)).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    const prior = [
      ['adapters/pms-read-tail-r17-sql.ts', 'PMS_WRITE_CONTEXT_SQL'],
      ['adapters/pms-read-tail-r18-sql.ts', 'PMS_BOOKING_PAYMENT_BY_ID_SQL'],
      ['adapters/pms-read-tail-r19-sql.ts', 'PMS_IMPORT_BATCHES_FROM_SQL'],
      ['adapters/pms-read-tail-r20-sql.ts', 'PMS_PROPERTIES_PROJECTION_SQL'],
      ['adapters/pms-read-tail-r21-sql.ts', 'PMS_RECIPIENT_CROSS_JOIN_SQL'],
      ['adapters/pms-read-tail-r22-sql.ts', 'PMS_STRIPE_REFUND_COLUMNS_SQL'],
      ['adapters/pms-read-tail-r23-sql.ts', 'PMS_SCOPED_RECORDS_SQL'],
      ['adapters/pms-read-tail-r24-sql.ts', 'PMS_RECORDS_PROJECTION_PREFIX_SQL'],
    ];
    for (const [file, constant] of prior) {
      expect((await src(file)).split(constant).length - 1).toBe(1);
    }
  });

  it('lowers the pms owner SELECT KPI strictly below the r24 after=420 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(258);
    expect(count).toBeLessThan(420);
  });
});
