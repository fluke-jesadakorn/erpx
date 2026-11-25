import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const exists = async (name: string) => {
  try { await src(name); return true; } catch { return false; }
};

// r46 was the owner-local renderer for the two-column point reads. All four
// variants were retired to the native command-plan owner and the helper file
// was deleted. This suite is now the retirement check that no TypeScript copy
// or renderer survives.
const RETIRED_SOURCE = 'adapters/pms-read-tail-r46-sql.ts';
const RETIRED_HEADS = [
  'SELECT amount_minor,allocations_json FROM pms_booking_refund_requests WHERE payment_id=?',
  'SELECT id,request_hash FROM pms_stripe_payments WHERE id=?',
  'SELECT company_id,branch_id FROM pms_properties WHERE id=?',
  'SELECT property_id,reservation_id FROM pms_booking_tokens WHERE token_hash=? AND expires_at>?',
] as const;
const RETIRED_KINDS = [
  'PMS_RD_REFUND_REQUEST_ALLOCATIONS',
  'PMS_RD_STRIPE_PAYMENT_KEY',
  'PMS_RD_PROPERTY_SCOPE',
  'PMS_RD_BOOKING_TOKEN',
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r46 retirement after the native point-read cut', () => {
  it('deletes the renderer file and all inline TypeScript heads', async () => {
    expect(await exists(RETIRED_SOURCE)).toBe(false);
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      for (const head of RETIRED_HEADS) {
        // The property FOR KEY SHARE sibling keeps the identical non-lock
        // prefix by design and was deliberately left in the caller.
        if (file.endsWith('property-purchase-boundary.ts') && head === RETIRED_HEADS[2]) continue;
        expect(text, file).not.toContain(head);
      }
      expect(text, file).not.toContain('pmsTwoColumnPointReadSql');
    }
  });

  it('keeps the retired bytes only in the native plan registry and bridge', async () => {
    const rust = await src('adapters/native_read_sql.rs');
    for (const head of RETIRED_HEADS) expect(rust).toContain(head);
    for (const kind of RETIRED_KINDS) expect(rust).toContain('pub const ' + kind + ':');
    const bridge = await src('adapters/native-pms-read.ts');
    for (const kind of RETIRED_KINDS) expect(bridge).toContain('export const ' + kind + ' =');
    const consumers = [
      ['adapters/pms-booking-refunds.ts', 'PMS_RD_REFUND_REQUEST_ALLOCATIONS'],
      ['adapters/pms-stripe.ts', 'PMS_RD_STRIPE_PAYMENT_KEY'],
      ['adapters/property-purchase-boundary.ts', 'PMS_RD_PROPERTY_SCOPE'],
      ['adapters/pms-booking.ts', 'PMS_RD_BOOKING_TOKEN'],
    ] as const;
    for (const [file, kind] of consumers) expect(await src(file)).toContain(kind);
  });

  it('leaves lock reads, feature/native reads and r17-r45 helpers untouched', async () => {
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect(await src('adapters/property-purchase-boundary.ts')).toContain('"SELECT company_id,branch_id FROM pms_properties WHERE id=? FOR KEY SHARE"');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(60);
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 220 to 219', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(214);
    expect(bare).toBe(215);
    expect(spaced).toBeLessThan(220);
  });
});
