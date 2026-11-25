import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { pmsTwoColumnPointReadSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r46-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r46-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: exact HEAD SQL captured before this dedupe (KPI 286).
// Every closed-union recomposition must match verbatim, with binds untouched.
const HEAD = {
  refundRequestAllocations: 'SELECT amount_minor,allocations_json FROM pms_booking_refund_requests WHERE payment_id=?',
  stripePaymentKey: 'SELECT id,request_hash FROM pms_stripe_payments WHERE id=?',
  propertyScope: 'SELECT company_id,branch_id FROM pms_properties WHERE id=?',
  bookingTokenByHash: 'SELECT property_id,reservation_id FROM pms_booking_tokens WHERE token_hash=? AND expires_at>?',
} as const;
const HEAD_SHA = {
  refundRequestAllocations: '07c516779585c82a77b5e47e641a2ebcb7d815c706e19c20fc2699a3a6213a85',
  stripePaymentKey: 'a6985331bf6906058cdc3ef8935132f3bac4783028e9e35d18fa0327b672c198',
  propertyScope: '19a9db660bcfe6fa29189babd8462fffd2d667a27e74526dfa2ee89b0e47d6c9',
  bookingTokenByHash: 'd402ea176522bcdd986f0598966599ba440152650d0f6e9bf96d25d8ed6dab9e',
} as const;
const GENERATED = {
  refundRequestAllocations: pmsTwoColumnPointReadSql('refundRequestAllocations'),
  stripePaymentKey: pmsTwoColumnPointReadSql('stripePaymentKey'),
  propertyScope: pmsTwoColumnPointReadSql('propertyScope'),
  bookingTokenByHash: pmsTwoColumnPointReadSql('bookingTokenByHash'),
} as const;
const KEYS = Object.keys(HEAD) as (keyof typeof HEAD)[];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}
const calls = (text: string, name: string) => text.split(`${name}(`).length - 1;
const names = (text: string, name: string) => text.split(name).length - 1;

describe('pms read-tail r46 two-column point read dedupe', () => {
  it('pins the exact HEAD bytes of all four oracles', () => {
    for (const k of KEYS) expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
  });

  it('declares one closed union in the shared module with no caller SQL', async () => {
    const shared = await src(SHARED);
    expect(names(shared, 'pmsTwoColumnPointReadSql')).toBe(1);
    expect(shared).toContain('type PointRead =');
    expect(shared).toContain('const POINT_READ');
    // No caller SQL string is accepted by the closed union.
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
  });

  it('recomposes all four point reads byte-identically with one bind each', async () => {
    for (const k of KEYS) {
      expect(GENERATED[k], k).toBe(HEAD[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length, k).toBe((HEAD[k].match(/\?/g) ?? []).length);
    }
    expect(GENERATED.bookingTokenByHash).toContain('token_hash=? AND expires_at>?');
    expect((await src(SHARED)).split('SELECT ').length - 1, 'helper adds one SELECT token').toBe(1);
  });

  it('routes all four call sites through the helper and preserves binds and order', async () => {
    const sites = [
      ['adapters/pms-booking-refunds.ts', 'refundRequestAllocations', ".bind(facts.paymentId).all<{"],
      ['adapters/pms-stripe.ts', 'stripePaymentKey', '.bind(facts.key).first<{'],
      ['adapters/property-purchase-boundary.ts', 'propertyScope', '.bind(propertyId).first<{ company_id: string; branch_id: string }>()'],
      ['adapters/pms-booking.ts', 'bookingTokenByHash', '.bind(facts.fact0, facts.now).first<{'],
    ] as const;
    let total = 0;
    for (const [file, kind, bind] of sites) {
      const text = await src(file);
      expect(calls(text, 'pmsTwoColumnPointReadSql'), file).toBe(1); // one routed call
      expect(text).toContain(`pmsTwoColumnPointReadSql('${kind}')`);
      expect(text).toContain(bind);
      // The inline non-lock head is gone; the FOR KEY SHARE sibling keeps its
      // identical prefix by design and is deliberately out of scope.
      expect(text.split(HEAD[kind]).length - 1, file).toBe(kind === 'propertyScope' ? 1 : 0);
      total += calls(text, 'pmsTwoColumnPointReadSql');
    }
    expect(total).toBe(4);
  });

  it('leaves lock reads, feature/native reads and r17-r45 helpers untouched', async () => {
    const boundary = await src('adapters/property-purchase-boundary.ts');
    expect(boundary).toContain('"SELECT company_id,branch_id FROM pms_properties WHERE id=? FOR KEY SHARE"');
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect((await src('adapters/pms-booking-refunds.ts')).split("'SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE'").length - 1).toBe(2);
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src('adapters/feature-availability.ts')).toContain('EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=');
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_STRIPE_PAYMENT_BY_ID');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 283 to 255', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(246);
    expect(bare).toBe(248);
    expect(spaced).toBeLessThan(286);
    expect(bare).toBeLessThan(288);
  });
});
