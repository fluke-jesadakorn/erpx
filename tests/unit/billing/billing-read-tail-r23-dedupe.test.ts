import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingPendingInboxSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r23-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-billing/src/adapters/${name}`, import.meta.url), 'utf8');

/** Byte-identical originals of the two pending stripe_inbox drain reads. */
const MATCHED = `SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$2) ORDER BY created_at,id LIMIT 50`;
const LEGACY = `SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND NOT EXISTS(SELECT 1 FROM billing_accounts a WHERE a.customer_id=stripe_inbox.payload_json::jsonb #>> '{data,object,customer}') LIMIT 20`;

/** Parameterized customer-correlation tails; the shared prefix lives in the helper. */
const MATCHED_TAIL = `payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$2) ORDER BY created_at,id LIMIT 50`;
const LEGACY_TAIL = `NOT EXISTS(SELECT 1 FROM billing_accounts a WHERE a.customer_id=stripe_inbox.payload_json::jsonb #>> '{data,object,customer}') LIMIT 20`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r23 parameterized family dedupe', () => {
  it('renders both pending-inbox reads byte-identically to the original inline SQL', () => {
    expect(billingPendingInboxSql(MATCHED_TAIL)).toBe(MATCHED);
    expect(billingPendingInboxSql(LEGACY_TAIL)).toBe(LEGACY);
  });

  it('routes both call sites through the shared helper with no residual inline SQL', async () => {
    const jobs = await src('billing-jobs.ts');
    expect(jobs.split("from './billing-read-tail-r23-sql'").length - 1).toBe(1);
    expect(jobs.split('billingPendingInboxSql(').length - 1).toBe(2);
    for (const inline of [
      `"SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND payload_json`,
      `"SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND NOT EXISTS`,
    ]) expect(jobs).not.toContain(inline);
    expect(jobs).toContain('billingPendingInboxSql("payload_json::jsonb');
    expect(jobs).toContain('billingPendingInboxSql("NOT EXISTS(');
  });

  it('keeps the helper read-only and leaves advisory/FOR UPDATE reads untouched', async () => {
    for (const fragment of [MATCHED, LEGACY]) {
      expect(fragment).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const billing = await src('billing.ts');
    expect(billing).toContain('BILLING_ADVISORY_XACT_LOCK_SQL');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain('"SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked"');
    expect(jobs).toContain('BILLING_ADVISORY_UNLOCK_SQL');
  });

  it('lowers the billing owner SELECT KPI strictly below the recorded BEFORE=63 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(63);
    expect(count).toBe(55); // r28 left 57; r29 deduped the point-read-by-id family (receipt, invoice, job).
  });
});
