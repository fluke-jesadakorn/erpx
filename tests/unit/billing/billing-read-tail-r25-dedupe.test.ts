import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const src = (name: string) => readFile(new URL(`../../../modules/billing/server/src/adapters/${name}`, import.meta.url), 'utf8');

/** r37 retires the shared TypeScript advisory-lock text file: both texts are
 * now allow-listed Billing owner kinds, executed on the caller's PoolClient. */
const XACT_LOCK = `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`;
const UNLOCK = `SELECT pg_advisory_unlock(hashtextextended($1,0))`;

describe('billing advisory lock text head retirement (r37)', () => {
  it('no longer ships a TypeScript advisory-lock SQL file', async () => {
    await expect(src('billing-read-tail-r25-sql.ts')).rejects.toThrow();
  });

  it('authors both advisory texts byte-identically in the owner crate', async () => {
    const adapter = await src('billing_owner_write.rs');
    expect(adapter).toContain(`const ADVISORY_XACT_LOCK_SQL: &str = r####"${XACT_LOCK}"####;`);
    expect(adapter).toContain(`const ADVISORY_UNLOCK_SQL: &str = r####"${UNLOCK}"####;`);
    expect(XACT_LOCK).not.toBe(UNLOCK);
  });

  it('keeps both call sites on the same client and leaves the other locks intact', async () => {
    const billing = await src('billing.ts');
    const jobs = await src('billing-jobs.ts');
    expect(billing.split("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])").length - 1).toBe(2);
    expect(jobs.split("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])").length - 1).toBe(1);
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(\'billing-plans\',0))"');
    expect(jobs).toContain('"SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked"');
  });
});
