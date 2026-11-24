import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { BILLING_ADVISORY_XACT_LOCK_SQL, BILLING_ADVISORY_UNLOCK_SQL } from '../../../packages/server-billing/src/adapters/billing-read-tail-r25-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-billing/src/adapters/${name}`, import.meta.url), 'utf8');

/** Byte-identical originals captured from the pre-r25 owner adapters. */
const XACT_LOCK = `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`;
const UNLOCK = `SELECT pg_advisory_unlock(hashtextextended($1,0))`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing advisory lock text family dedupe', () => {
  it('renders both advisory-lock texts byte-identically to the originals', () => {
    expect(BILLING_ADVISORY_XACT_LOCK_SQL).toBe(XACT_LOCK);
    expect(BILLING_ADVISORY_UNLOCK_SQL).toBe(UNLOCK);
  });

  it('routes both call sites through the shared constants with their own binds', async () => {
    const billing = await src('billing.ts');
    expect(billing.split("from './billing-read-tail-r25-sql'").length - 1).toBe(1);
    expect(billing.split('BILLING_ADVISORY_XACT_LOCK_SQL,[value0]').length - 1).toBe(2);
    expect(billing).not.toContain('"SELECT pg_advisory_xact_lock(hashtextextended($1,0))"');

    const jobs = await src('billing-jobs.ts');
    expect(jobs.split("from './billing-read-tail-r25-sql'").length - 1).toBe(1);
    expect(jobs.split('BILLING_ADVISORY_UNLOCK_SQL,[v0]').length - 1).toBe(1);
    expect(jobs).not.toContain('"SELECT pg_advisory_unlock(hashtextextended($1,0))"');
  });

  it('leaves FOR UPDATE and all other statements untouched', async () => {
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(\'billing-plans\',0))"');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain('"SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked"');
  });

  it('lowers the billing owner SELECT KPI strictly below the recorded BEFORE=63 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(63);
    expect(count).toBe(55); // r28 left 57; r29 deduped the point-read-by-id family (receipt, invoice, job).
  });
});
