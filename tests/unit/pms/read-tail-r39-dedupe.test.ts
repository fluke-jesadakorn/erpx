import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  pmsHomeGuestAccountSumSql,
  pmsJournalIdentityUnionSql,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r39-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r39-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

const SUMS = {
  receivable: "SELECT sum((r.data_json::jsonb->>'due')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='guest_account'",
  advance: "SELECT sum((r.data_json::jsonb->>'advance')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='guest_account'",
  deposit: "SELECT sum((r.data_json::jsonb->>'deposit')::bigint) FROM pms_records r WHERE r.property_id=p.id AND r.kind='guest_account'",
} as const;
const JOURNALS = {
  identity: "SELECT p.company_id,d.journal_id id FROM pms_postings d JOIN pms_properties p ON p.id=d.property_id WHERE d.journal_id IS NOT NULL UNION SELECT company_id,opening_journal_id FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL",
  sourceImage: "SELECT p.company_id,d.journal_id id,jsonb_build_object('property',to_jsonb(p),'posting',to_jsonb(d)) source_image FROM pms_postings d JOIN pms_properties p ON p.id=d.property_id WHERE d.journal_id IS NOT NULL UNION ALL SELECT company_id,opening_journal_id,to_jsonb(d) FROM pms_import_batches d WHERE state='applied' AND opening_journal_id IS NOT NULL",
} as const;

// Byte oracles: exact SQL substrings captured at HEAD before this dedupe. The
// SHA-256 values are the pre-change text; every closed-union recomposition must
// match byte-for-byte and keep the same bound parameters in the same order.
const ORIGINALS = [
  { file: 'adapters/pms/home.ts', kind: 'receivable', full: SUMS.receivable, sha256: 'bf3b659667c7f8386faf7d249378a4166a46b9638d0a7c7c22415143d05adb63' },
  { file: 'adapters/pms/home.ts', kind: 'advance', full: SUMS.advance, sha256: 'b299e72952363bdfa668085c95045db2fd861b23f297d248446aa0ad8e03da76' },
  { file: 'adapters/pms/home.ts', kind: 'deposit', full: SUMS.deposit, sha256: '3bc02e3936ecda491187e3f859bb89ba43113bb058e02ed188c4133ad8d07534' },
  { file: 'adapters/finance-record-sources.ts', kind: 'identity', full: JOURNALS.identity, sha256: 'c0621762a3a9bc62b93d1af7459fab69681ca17d5f7f30f8b842c4f7cdf748f8' },
  { file: 'adapters/finance-record-sources.ts', kind: 'sourceImage', full: JOURNALS.sourceImage, sha256: '22d8ccd14ef760c6189a2d1cc08923c5f54fbad48c56a31c36c48d64ba5334b1' },
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r39 home-sum and journal-union dedupe', () => {
  it('declares each closed union exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split('pmsHomeGuestAccountSumSql').length - 1).toBe(1);
    expect(shared.split('pmsJournalIdentityUnionSql').length - 1).toBe(1);
    expect(shared).toContain('const GUEST_ACCOUNT_SUM_FIELD');
    expect(shared).toContain('const JOURNAL_IDENTITY_HEAD');
    expect(shared).toContain('const IMPORT_JOURNAL_IDENTITY_HEAD');
  });

  it('generates all five byte-identical heads from the closed variants', () => {
    for (const oracle of ORIGINALS) {
      const generated = oracle.kind in SUMS
        ? pmsHomeGuestAccountSumSql(oracle.kind as keyof typeof SUMS)
        : pmsJournalIdentityUnionSql(oracle.kind as keyof typeof JOURNALS);
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(generated).toBe(oracle.full);
      expect((generated.match(/\?/g) ?? []).length).toBe((oracle.full.match(/\?/g) ?? []).length);
    }
  });

  it('routes all five call sites through the shared unions and removes inline heads', async () => {
    // r41 folds the three home guest-account sums into its property-scope union.
    const r41 = await src('adapters/pms-read-tail-r41-sql.ts');
    expect(r41.split('${pmsHomeGuestAccountSumSql(').length - 1).toBe(3); // three guest-account money columns
    const home = await src('adapters/pms/home.ts');
    for (const sum of Object.values(SUMS)) expect(home.split(sum).length - 1).toBe(0);

    const fin = await src('adapters/finance-record-sources.ts');
    expect(fin.split('pmsJournalIdentityUnionSql(').length - 1).toBe(2); // identity + source image
    for (const journal of Object.values(JOURNALS)) expect(fin.split(journal).length - 1).toBe(0);
    expect(fin).toContain('import { pmsJournalIdentityUnionSql } from "./pms-read-tail-r39-sql";');
  });

  it('keeps locks, the native read registry and the r17-r38 fragments untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect(await src('adapters/pms-booking-refunds.ts')).toContain("'SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 317 to 313', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(237);
    expect(bare).toBe(238);
    expect(spaced).toBeLessThan(317);
  });
});
