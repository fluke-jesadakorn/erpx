import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { pmsPosPropertyConfigSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r33-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r33-sql.ts';
const CALL_SITE = 'adapters/pos-stripe-facts.ts';

const BOOKING = "SELECT p.config_json FROM pms_properties p JOIN pms_booking_payments b ON b.property_id=p.id WHERE b.company_id=? ORDER BY b.created_at LIMIT 1";
const STRIPE = "SELECT p.config_json FROM pms_properties p JOIN pms_stripe_payments m ON m.property_id=p.id WHERE m.company_id=? ORDER BY m.created_at LIMIT 1";

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r33 parameterized POS property-config family dedupe', () => {
  it('declares the shared fragment exactly once', async () => {
    expect((await src(SHARED)).split('pmsPosPropertyConfigSql').length - 1).toBe(1);
  });

  it('generates byte-identical SQL for both closed source variants', () => {
    expect(pmsPosPropertyConfigSql('booking')).toBe(BOOKING);
    expect(pmsPosPropertyConfigSql('stripe')).toBe(STRIPE);
  });

  it('routes both call sites through the shared fragment and removes the inline literals', async () => {
    const text = await src(CALL_SITE);
    expect(text.split('pmsPosPropertyConfigSql').length - 1).toBe(3); // import + 2 uses
    expect(text).not.toContain(BOOKING);
    expect(text).not.toContain(STRIPE);
  });

  it('keeps the fragment read-only and leaves locks, r17-r32 fragments and the registry untouched', async () => {
    const shared = await src(SHARED);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const call = await src(CALL_SITE);
    expect(call).toContain("from './native-pms-read'");
    expect(call).toContain('PMS_RD_POS_STRIPE_PAYMENT');
    expect(call).toContain('PMS_RD_POS_BOOKING_PAYMENT');
    expect(await src('adapters/pms-stripe.ts')).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    for (const n of [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI strictly below the r32 after=345 baseline', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(305);
    expect(bare).toBe(307);
    expect(spaced).toBeLessThan(345);
  });
});
