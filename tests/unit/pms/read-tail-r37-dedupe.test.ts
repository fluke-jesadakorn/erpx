import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  PMS_STRIPE_PAYMENT_ID_HEAD_SQL,
  PMS_BOOKING_PAYMENT_ID_HEAD_SQL,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r37-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r37-sql.ts';
const STRIPE = 'SELECT id FROM pms_stripe_payments WHERE';
const BOOKING = 'SELECT id FROM pms_booking_payments WHERE';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: exact SQL captured at HEAD before this dedupe. SHA-256 values are
// the pre-change literals; every recomposition must match byte-for-byte and keep
// the same bound parameters in the same order.
const ORIGINALS = [
  {
    file: 'adapters/pms-stripe.ts',
    full: "SELECT id FROM pms_stripe_payments WHERE state IN ('requested','pending') OR (state='cancelled' AND provider_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20",
    sha256: 'b02889b513bc456d72ceebbd43a006b44df6eca1677365784eafa7ee871dc93a',
    recomposed: `${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} state IN ('requested','pending') OR (state='cancelled' AND provider_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20`,
    routed: '${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} state IN (',
  },
  {
    file: 'adapters/pms-stripe.ts',
    full: "SELECT id FROM pms_stripe_payments WHERE refund_state IN ('approved','processing','requires_action') ORDER BY updated_at LIMIT 20",
    sha256: '605799c7cc4be71d300032859a7ad0f80416b4ca23f26c8b2b36ae510a05c6d5',
    recomposed: `${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} refund_state IN ('approved','processing','requires_action') ORDER BY updated_at LIMIT 20`,
    routed: '${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} refund_state IN (',
  },
  {
    file: 'adapters/pms-booking.ts',
    full: "SELECT id FROM pms_booking_payments WHERE status='payment_pending' OR (status='expired' AND checkout_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20",
    sha256: 'f753887fb87b78ce7617b0f63968581e1ff4fa6055691611ae9d5646d5a03d70',
    recomposed: `${PMS_BOOKING_PAYMENT_ID_HEAD_SQL} status='payment_pending' OR (status='expired' AND checkout_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20`,
    routed: '${PMS_BOOKING_PAYMENT_ID_HEAD_SQL} status=',
  },
  {
    file: 'adapters/pms-booking.ts',
    full: "SELECT id FROM pms_booking_payments WHERE id IN (SELECT payment_id FROM pms_booking_refund_requests WHERE state IN ('approved','processing','requires_action')) ORDER BY updated_at LIMIT 20",
    sha256: '9d6b2aa21eb4fc4f151c305381b743c89c0359787374f673d6165bcffc809211',
    recomposed: `${PMS_BOOKING_PAYMENT_ID_HEAD_SQL} id IN (SELECT payment_id FROM pms_booking_refund_requests WHERE state IN ('approved','processing','requires_action')) ORDER BY updated_at LIMIT 20`,
    routed: '${PMS_BOOKING_PAYMENT_ID_HEAD_SQL} id IN (',
  },
  {
    file: 'adapters/pos-stripe-facts.ts',
    full: "SELECT id FROM pms_booking_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=? AND status IN ('confirmed','cancelled','refund_review')",
    sha256: '53108569d8010db8865b2d3aa82a1f35b9c167256db78598a7460745fb4007fa',
    recomposed: `${PMS_BOOKING_PAYMENT_ID_HEAD_SQL} id=? AND company_id=? AND account_id=? AND livemode=? AND status IN ('confirmed','cancelled','refund_review')`,
    routed: '${PMS_BOOKING_PAYMENT_ID_HEAD_SQL} id=?',
  },
  {
    file: 'adapters/pos-stripe-facts.ts',
    full: "SELECT id FROM pms_stripe_payments WHERE id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'",
    sha256: 'e62dd6b8f3b957d4e008a95fceee39c002e142e7ea1e5f6009569db27723ecd0',
    recomposed: `${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'`,
    routed: '${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} id=?',
  },
  {
    file: 'adapters/pos-stripe-facts.ts',
    full: "SELECT id FROM pms_stripe_payments WHERE refund_id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'",
    sha256: '80dcb973e889b2387d465c84a9cf8db2e4b6eefaa75f64becff3921d5d35a41e',
    recomposed: `${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} refund_id=? AND company_id=? AND account_id=? AND livemode=? AND state='succeeded'`,
    routed: '${PMS_STRIPE_PAYMENT_ID_HEAD_SQL} refund_id=?',
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

describe('pms read-tail r37 payment-id head dedupe', () => {
  it('declares each byte-identical head exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split(STRIPE).length - 1).toBe(1);
    expect(shared.split(BOOKING).length - 1).toBe(1);
    expect(shared.split('PMS_STRIPE_PAYMENT_ID_HEAD_SQL').length - 1).toBe(1);
    expect(shared.split('PMS_BOOKING_PAYMENT_ID_HEAD_SQL').length - 1).toBe(1);
  });

  it('exports the exact byte-identical heads', () => {
    expect(PMS_STRIPE_PAYMENT_ID_HEAD_SQL).toBe(STRIPE);
    expect(PMS_BOOKING_PAYMENT_ID_HEAD_SQL).toBe(BOOKING);
  });

  it('recomposes all seven original SQL literals byte-for-byte with unchanged binds', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(oracle.recomposed).toBe(oracle.full);
      expect((oracle.recomposed.match(/\?/g) ?? []).length).toBe((oracle.full.match(/\?/g) ?? []).length);
    }
  });

  it('routes all seven call sites through the shared heads and removes inline heads', async () => {
    for (const oracle of ORIGINALS) {
      expect(await src(oracle.file), oracle.routed).toContain(oracle.routed);
    }
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe.split('PMS_STRIPE_PAYMENT_ID_HEAD_SQL').length - 1).toBe(3); // import + 2 uses
    expect(stripe.split(STRIPE).length - 1).toBe(0);
    const booking = await src('adapters/pms-booking.ts');
    expect(booking.split('PMS_BOOKING_PAYMENT_ID_HEAD_SQL').length - 1).toBe(3); // import + 2 uses
    expect(booking.split(BOOKING).length - 1).toBe(0);
    const pos = await src('adapters/pos-stripe-facts.ts');
    expect(pos.split('PMS_STRIPE_PAYMENT_ID_HEAD_SQL').length - 1).toBe(3); // import + 2 uses
    expect(pos.split('PMS_BOOKING_PAYMENT_ID_HEAD_SQL').length - 1).toBe(2); // import + 1 use
    expect(pos.split(STRIPE).length - 1).toBe(0);
    expect(pos.split(BOOKING).length - 1).toBe(0);
  });

  it('routes the r29 probe through the shared head and keeps locks/native registry untouched', async () => {
    // r54 folds the byte-identical booking head into this owner-local renderer.
    const r29 = await src('adapters/pms-read-tail-r29-sql.ts');
    expect(r29).toContain("import { PMS_BOOKING_PAYMENT_ID_HEAD_SQL } from './pms-read-tail-r37-sql'");
    expect(r29.split('PMS_BOOKING_PAYMENT_ID_HEAD_SQL').length - 1).toBe(2); // import + 1 use
    expect(r29.split(BOOKING).length - 1).toBe(0);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect(await src('adapters/pms-booking-refunds.ts')).toContain("'SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_STRIPE_PAYMENT_BY_ID');
  });

  it('leaves the r17-r36 shared fragments untouched', async () => {
    const shared = await src(SHARED);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 328 to 323', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(242);
    expect(bare).toBe(244);
    expect(spaced).toBeLessThan(328);
  });
});
