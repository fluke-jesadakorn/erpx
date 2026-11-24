import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { PMS_NEEDS_RECEIPT_RECOVERY_SQL, pmsRefundPaymentJoinHeadSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r34-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r34-sql.ts';

const F = PMS_NEEDS_RECEIPT_RECOVERY_SQL;
const HEADS = {
  requestId: 'SELECT r.id FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id',
  brief: 'SELECT r.id,r.amount_minor,r.state,r.reason FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id',
  count: 'SELECT count(*) n FROM pms_booking_refund_requests r\n    JOIN pms_booking_payments p ON p.id=r.payment_id',
  detail: 'SELECT r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error,r.reason FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id',
  detailReceipt: `SELECT r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error, ${F} FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id`,
  property: 'SELECT r.id,p.property_id FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id',
  fullReceipt: `SELECT r.id,p.id payment_id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.last_error,r.reason,r.created_at, ${F} FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id`,
} as const;

const SITES: { file: string; variant: keyof typeof HEADS; tail: string; full: string }[] = [
  { file: 'adapters/pos-stripe-facts.ts', variant: 'requestId', tail: ' WHERE r.provider_id=? AND p.company_id=? AND p.account_id=? AND p.livemode=?', full: `${HEADS.requestId} WHERE r.provider_id=? AND p.company_id=? AND p.account_id=? AND p.livemode=?` },
  { file: 'adapters/pms-booking.ts', variant: 'brief', tail: ' WHERE p.reservation_id=? ORDER BY r.created_at DESC', full: `${HEADS.brief} WHERE p.reservation_id=? ORDER BY r.created_at DESC` },
  { file: 'adapters/hospitality-day-controls.ts', variant: 'count', tail: " WHERE p.property_id=? AND p.company_id=?\n    AND (r.state='failed' OR (r.state IN ('approved','processing','requires_action') AND r.last_error IS NOT NULL))\n    AND to_char(to_timestamp(r.created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?", full: `${HEADS.count} WHERE p.property_id=? AND p.company_id=?\n    AND (r.state='failed' OR (r.state IN ('approved','processing','requires_action') AND r.last_error IS NOT NULL))\n    AND to_char(to_timestamp(r.created_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')<=?` },
  { file: 'adapters/pms/record-details.ts', variant: 'detail', tail: ' WHERE r.id=? AND p.company_id=?', full: `${HEADS.detail} WHERE r.id=? AND p.company_id=?` },
  { file: 'adapters/pms.ts', variant: 'detailReceipt', tail: ' WHERE p.company_id=? AND p.property_id=? ORDER BY r.created_at DESC LIMIT 100', full: `${HEADS.detailReceipt} WHERE p.company_id=? AND p.property_id=? ORDER BY r.created_at DESC LIMIT 100` },
  { file: 'adapters/pms-booking-refunds.ts', variant: 'property', tail: " WHERE (r.id=? OR p.id=?) AND p.company_id=? AND r.state='requested' ORDER BY r.created_at LIMIT 1", full: `${HEADS.property} WHERE (r.id=? OR p.id=?) AND p.company_id=? AND r.state='requested' ORDER BY r.created_at LIMIT 1` },
  { file: 'adapters/pms-booking-refunds.ts', variant: 'fullReceipt', tail: ' WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100', full: `${HEADS.fullReceipt} WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100` },
];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r34 refund/payment join-head dedupe', () => {
  it('declares the Rank 1 fragment once and the Rank 2 helper once', async () => {
    const shared = await src(SHARED);
    expect(shared.split('PMS_NEEDS_RECEIPT_RECOVERY_SQL').length - 1).toBe(3); // declaration + 2 receipt variants
    expect(shared.split('pmsRefundPaymentJoinHeadSql').length - 1).toBe(1);
  });

  it('generates byte-identical SQL for all seven closed head variants', () => {
    for (const [variant, want] of Object.entries(HEADS)) {
      expect(pmsRefundPaymentJoinHeadSql(variant as keyof typeof HEADS)).toBe(want);
    }
  });

  it('keeps the recovery fragment byte-identical at both receipt sites', () => {
    for (const full of [SITES[4].full, SITES[6].full]) {
      expect(full.split(F).length - 1).toBe(1);
      expect(full).toContain(`, ${F} FROM pms_booking_refund_requests r`);
    }
    expect(F.split('SELECT 1').length - 1).toBe(1);
  });

  it('recomposes the original full SQL at every site with caller-owned WHERE/ORDER/LIMIT', () => {
    for (const site of SITES) {
      expect(pmsRefundPaymentJoinHeadSql(site.variant) + site.tail).toBe(site.full);
    }
  });

  it('routes all seven call sites through the helper and removes the inline heads', async () => {
    for (const site of SITES) {
      const text = await src(site.file);
      expect(text).toContain(`pmsRefundPaymentJoinHeadSql('${site.variant}')`);
      expect(text).not.toContain(HEADS[site.variant]);
    }
  });

  it('leaves locks, r17-r33 fragments and the native read registry untouched', async () => {
    const shared = await src(SHARED);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src('adapters/pms-stripe.ts')).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(await src('adapters/pms-booking-refunds.ts')).toContain("'SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32, 33]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI strictly below the r33 after=344 baseline', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(310);
    expect(bare).toBe(312);
    expect(spaced).toBeLessThan(344);
  });
});
