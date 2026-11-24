import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SQL = 'SELECT * FROM pms_booking_payments WHERE id=?';
const SHARED = 'adapters/pms-read-tail-r18-sql.ts';
const CONST = 'PMS_BOOKING_PAYMENT_BY_ID_SQL';
const CALL_SITES = [
  'adapters/pms-booking.ts',
  'adapters/pms-booking-refunds.ts',
];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r18 byte-identical point-read dedupe', () => {
  it('declares the shared SQL text exactly once', async () => {
    expect((await src(SHARED)).split(SQL).length - 1).toBe(1);
  });

  it('routes every call site through the shared fragment and leaves no inline literal', async () => {
    let usages = 0;
    for (const name of CALL_SITES) {
      const text = await src(name);
      usages += text.split(CONST).length - 1;
      expect(text).not.toContain(`'${SQL}'`);
    }
    // one import + 3 call sites in pms-booking.ts, one import + 1 call site in refunds = 6
    expect(usages).toBe(6);
  });

  it('keeps the fragment read-only and leaves FOR UPDATE locks untouched', async () => {
    expect(SQL).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const booking = await src('adapters/pms-booking.ts');
    expect(booking).toContain("'SELECT * FROM pms_booking_payments WHERE id=? FOR UPDATE'");
    const refunds = await src('adapters/pms-booking-refunds.ts');
    expect(refunds).toContain('"SELECT status FROM pms_booking_payments WHERE id=? FOR UPDATE"');
  });

  it('lowers the pms owner SELECT KPI strictly below the r17 after=490 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    // r19 deduped the two 4x import-batches/postings projection fragments below r18 after=487;
    // r20 deduped the 3x pms_properties and 3x pms_records projections below r19 after=481.
    // r21 deduped the 4x parameterized recipient cross-join fragment below r20 after=477.
    expect(count).toBe(347);
    expect(count).toBeLessThan(490);
  });
});
