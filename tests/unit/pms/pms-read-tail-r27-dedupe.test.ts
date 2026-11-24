import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r27-sql.ts';

const CTE = 'WITH purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} observed_at) ';

const BOOKING = 'adapters/pms-booking.ts';
const SOURCE = 'adapters/pms/source.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r27 purchase-clock CTE dedupe', () => {
  it('declares the shared CTE exactly once', async () => {
    expect((await src(SHARED)).split(CTE).length - 1).toBe(1);
  });

  it('routes both call sites through the shared CTE with no inline literal', async () => {
    for (const file of [BOOKING, SOURCE]) {
      const text = await src(file);
      expect(text.split('PMS_PURCHASE_CLOCK_CTE_SQL').length - 1).toBe(2); // import + use
      expect(text).not.toContain('WITH purchase_clock AS MATERIALIZED (SELECT');
    }
  });

  it('keeps the fragment read-only, leaves locks inline and does not re-dedupe r17-r26', async () => {
    expect(CTE).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
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
      ['adapters/pms-read-tail-r26-sql.ts', 'PMS_GUEST_ID_FROM_STAYS_SQL'],
    ];
    for (const [file, constant] of prior) {
      expect((await src(file)).split(constant).length - 1).toBe(1);
    }
  });

  it('lowers the pms owner SELECT KPI strictly below the r26 after=405 baseline', async () => {
    let spaced = 0;
    let bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBeLessThanOrEqual(358);
    expect(bare).toBeLessThanOrEqual(360);
    expect(spaced).toBeLessThan(405);
    expect(bare).toBeLessThan(407);
  });
});
