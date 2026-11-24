import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r26-sql.ts';

const IDS_MEMBER = '(SELECT jsonb_array_elements_text(?::jsonb))';
const CHOSEN = 'SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN ${PMS_IDS_JSONB_MEMBER_SQL}';
const GUEST_ACCOUNTS = "SELECT data_json::jsonb->>'guestId' FROM accounts";
const GUEST_STAYS = "SELECT data_json::jsonb->>'guestId' FROM stays";

const PMS = 'adapters/pms.ts';
const REPO = 'adapters/pms/read-repository.ts';
const SOCIAL = 'adapters/social-source-purchases.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r26 byte-identical id-source dedupe', () => {
  it('declares each shared SQL text exactly once', async () => {
    const shared = await src(SHARED);
    for (const sql of [IDS_MEMBER, CHOSEN, GUEST_ACCOUNTS, GUEST_STAYS]) {
      expect(shared.split(sql).length - 1).toBe(1);
    }
  });

  it('routes every call site through the shared fragments with no inline literal', async () => {
    const pms = await src(PMS);
    const repo = await src(REPO);
    const social = await src(SOCIAL);
    // imports + uses
    expect(pms.split('PMS_IDS_JSONB_MEMBER_SQL').length - 1).toBe(2); // import + 1 use
    expect(pms.split('PMS_GUEST_ID_FROM_ACCOUNTS_SQL').length - 1).toBe(0); // r28 native port removed all uses and the import
    expect(pms.split('PMS_GUEST_ID_FROM_STAYS_SQL').length - 1).toBe(0); // r28 native port removed all uses and the import
    expect(repo.split('PMS_CHOSEN_RECORDS_SQL').length - 1).toBe(3); // import + 2 uses
    expect(social.split('PMS_IDS_JSONB_MEMBER_SQL').length - 1).toBe(3); // import + 2 uses
    for (const [text, literals] of [
      [pms, [IDS_MEMBER, CHOSEN, GUEST_ACCOUNTS, GUEST_STAYS]],
      [repo, [IDS_MEMBER, CHOSEN, GUEST_ACCOUNTS, GUEST_STAYS]],
      [social, [IDS_MEMBER, CHOSEN, GUEST_ACCOUNTS, GUEST_STAYS]],
    ] as const) {
      for (const sql of literals) expect(text).not.toContain(sql);
    }
  });

  it('keeps the fragments read-only, leaves locks inline and does not re-dedupe r17-r25', async () => {
    for (const sql of [IDS_MEMBER, CHOSEN, GUEST_ACCOUNTS, GUEST_STAYS]) {
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
      ['adapters/pms-read-tail-r25-sql.ts', 'PMS_GUEST_STAYS_CHOSEN_MEMBER_SQL'],
    ];
    for (const [file, constant] of prior) {
      expect((await src(file)).split(constant).length - 1).toBe(1);
    }
  });

  it('lowers the pms owner SELECT KPI strictly below the r25 after=414 baseline', async () => {
    let spaced = 0;
    let bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(361);
    expect(bare).toBe(363);
    expect(spaced).toBeLessThan(414);
    expect(bare).toBeLessThan(416);
  });
});
