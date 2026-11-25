import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r29 point reads captured at BEFORE=57 (HEAD 8dba3a12). The
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const ORIGINALS = [
  {
    file: 'billing.ts',
    full: 'SELECT * FROM billing_receipts WHERE id=$1',
    columns: '*', table: 'billing_receipts', suffix: 'id=$1', binds: 1,
    sha256: '8c8cd9bb34ae58040d7b0e690a6d9d3cdd3b1ba79cdeb9e8342cec99f1df0fbe',
    routed: "billingPointReadSql('*','billing_receipts','id=$1')",
  },
  {
    file: 'billing-target.ts',
    full: 'SELECT id,total_satang,status,stripe_id FROM billing_invoices WHERE id=$1',
    columns: 'id,total_satang,status,stripe_id', table: 'billing_invoices', suffix: 'id=$1', binds: 1,
    sha256: 'bbdee083a1b236222b804747181ee9830a76583d07cb8d415b6a22ac2b87bbcb',
    routed: "billingPointReadSql('id,total_satang,status,stripe_id','billing_invoices','id=$1')",
  },
  {
    file: 'billing.ts',
    full: 'SELECT * FROM billing_jobs WHERE id=$1 AND holding_id=$2',
    columns: '*', table: 'billing_jobs', suffix: 'id=$1 AND holding_id=$2', binds: 2,
    sha256: '6f37e40564012526a62bae0c0c9e365f16d1f7cca68e8fb9e4e8996336c9fa8d',
    routed: "billingPointReadSql('*','billing_jobs','id=$1 AND holding_id=$2')",
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

describe('billing read-tail r29 point-read-by-id dedupe', () => {
  it('authors the point-read head exactly once as read-only text', async () => {
    const helper = await src('billing-read-tail-r29-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(0); // r39 routes the point read through r35
    expect(helper.split('billingPointReadSql').length - 1).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes all three point reads byte-for-byte with unchanged binds', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(billingPointReadSql(oracle.columns, oracle.table, oracle.suffix)).toBe(oracle.full);
      expect((oracle.full.match(/\$/g) ?? []).length).toBe(oracle.binds);
    }
  });

  it('removes the inline literals and routes every call site through the helper', async () => {
    const billing = await src('billing.ts');
    const target = await src('billing-target.ts');
    expect(billing).toContain("import {billingPointReadSql} from './billing-read-tail-r29-sql';");
    expect(target).toContain("import {billingPointReadSql} from './billing-read-tail-r29-sql';");
    for (const oracle of ORIGINALS) {
      const text = await src(oracle.file);
      expect(text.split(oracle.full).length - 1).toBe(0);
      expect(text.split(oracle.routed).length - 1).toBe(1);
    }
    expect(billing.split('billingPointReadSql(').length - 1).toBe(9); // r35 effective-month guard + provider total
    expect(target.split('billingPointReadSql(').length - 1).toBe(4);
  });

  it('routes the r15 invoice-by-id constant through r29 and keeps FOR UPDATE/advisory untouched', async () => {
    const r15 = await src('billing-read-tail-r15-sql.ts');
    expect(r15.split('SELECT ').length - 1).toBe(0);
    expect(r15).not.toContain('"SELECT * FROM billing_invoices WHERE id=$1"');
    expect(r15).not.toContain('BILLING_ACCOUNT_BY_HOLDING_SQL');
    expect(r15).not.toContain('BILLING_CUSTOMER_BY_HOLDING_SQL');
    const jobs = await src('billing-jobs.ts');
    expect(jobs.split('BILLING_INVOICE_BY_ID_SQL').length - 1).toBe(3); // import + two calls
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(billing).not.toContain('billing-read-tail-r25-sql');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=57 to 42', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(9);
    expect(bare).toBe(9);
    expect(spaced).toBeLessThan(48);
    expect(bare).toBeLessThan(48);
  });
});
