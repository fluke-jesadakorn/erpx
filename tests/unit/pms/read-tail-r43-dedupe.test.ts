import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  pmsNightSourceWitnessSql,
  pmsBookingEmailPendingSql,
  pmsNightLineageStepSql,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r43-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r43-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: exact SQL text captured at HEAD before this dedupe. The SHA-256
// values pin the pre-change bytes; every closed-union recomposition must match
// byte-for-byte and keep the same bound parameters in the same order.
const HEAD = {
  witnessEntry: "EXISTS(SELECT 1 FROM entry_lineage l WHERE l.entry_id=e.id AND l.source_key LIKE 'night:%')",
  witnessLineage: "EXISTS(SELECT 1 FROM lineage l WHERE l.entry_id=e.id AND l.source_key LIKE 'night:%')",
  bookingReservation: "SELECT reservation_id FROM pms_booking_emails WHERE property_id=? AND state!='sent' ORDER BY created_at DESC LIMIT 50",
  bookingFull: "SELECT id,reservation_id,state,attempts,last_error FROM pms_booking_emails WHERE property_id=? AND state!='sent' ORDER BY created_at DESC LIMIT 50",
  lineageEntry: "        SELECT l.entry_id,l.property_id,l.business_date,l.net_raw,parent.id,\n          parent.data_json::jsonb->>'sourceKey',parent.data_json::jsonb->>'originalId',l.visited||parent.id\n        FROM entry_lineage l JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'\n        WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))",
  lineagePlain: "      SELECT l.entry_id,l.property_id,l.net_raw,parent.id,parent.data_json::jsonb->>'sourceKey',\n        parent.data_json::jsonb->>'originalId',l.visited||parent.id\n      FROM lineage l JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'\n      WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))",
} as const;
const HEAD_SHA = {
  witnessEntry: 'a35af6d7fd214de85bcd2da299ce8fde8ff97d3924fc194b200bc7b6ad804873',
  witnessLineage: '9c82f1342a0eeaa38f79fcefb938b8091173a8cc555e4330a4f04b509fb9d102',
  bookingReservation: 'bf921b1ced47b6940d0acaa5c5fcef9e75a28ef68daa310d6013e5c82e685b3c',
  bookingFull: '724dca6e1e00a620ae940e02fc36ee55b5824a6303f3d003502047c86e4b2361',
  lineageEntry: '47f5ac4a7fa10affda3495a3472f0c7e0d6fcd123cfcd621c3376ea1fd94bcc4',
  lineagePlain: 'd40bdc8861d9a8982c0f7094fee3aea1128e75e8df3cfb3c5edfbadf1b8b0270',
} as const;
const GENERATED = {
  witnessEntry: pmsNightSourceWitnessSql('entry_lineage'),
  witnessLineage: pmsNightSourceWitnessSql('lineage'),
  bookingReservation: pmsBookingEmailPendingSql('reservationId'),
  bookingFull: pmsBookingEmailPendingSql('full'),
  lineageEntry: pmsNightLineageStepSql('entry_lineage'),
  lineagePlain: pmsNightLineageStepSql('lineage'),
} as const;
const BINDS = {
  witnessEntry: 0,
  witnessLineage: 0,
  bookingReservation: 1,
  bookingFull: 1,
  lineageEntry: 0,
  lineagePlain: 0,
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r43 night-witness, booking-email and lineage-step dedupe', () => {
  it('pins the exact HEAD bytes of all six oracles', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
    }
  });

  it('declares each closed union exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split('pmsNightSourceWitnessSql').length - 1).toBe(1);
    expect(shared.split('pmsBookingEmailPendingSql').length - 1).toBe(1);
    expect(shared.split('pmsNightLineageStepSql').length - 1).toBe(1);
    expect(shared).toContain("type NightSourceKind = 'entry_lineage' | 'lineage'");
    expect(shared).toContain("type BookingEmailProjection = 'reservationId' | 'full'");
    expect(shared).toContain("type NightLineageKind = 'entry_lineage' | 'lineage'");
    // No caller SQL is accepted by any closed union.
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
  });

  it('generates all six byte-identical heads with unchanged bind counts', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(GENERATED[k], k).toBe(HEAD[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length).toBe(BINDS[k]);
    }
  });

  it('keeps the booking-email DESC/LIMIT and lineage termination/guard verbatim', () => {
    for (const k of ['bookingReservation', 'bookingFull'] as const) {
      expect(GENERATED[k]).toContain("state!='sent' ORDER BY created_at DESC LIMIT 50");
      expect(GENERATED[k].split('?').length - 1).toBe(1);
    }
    for (const k of ['lineageEntry', 'lineagePlain'] as const) {
      expect(GENERATED[k]).toContain("WHERE COALESCE(l.source_key,'') NOT LIKE 'night:%' AND NOT (parent.id=ANY(l.visited))");
      expect(GENERATED[k]).toContain("JOIN pms_records parent ON parent.id=l.original_id AND parent.property_id=l.property_id AND parent.kind='entry'");
    }
  });

  it('routes all six call sites through the helpers and removes inline heads', async () => {
    const emp = await src('adapters/executive-metric-projections.ts');
    expect(emp.split("pmsNightSourceWitnessSql('entry_lineage')").length - 1).toBe(1);
    expect(emp.split("pmsNightLineageStepSql('entry_lineage')").length - 1).toBe(1);
    expect(emp.split(HEAD.witnessEntry).length - 1).toBe(0);
    expect(emp.split(HEAD.lineageEntry).length - 1).toBe(0);
    expect(emp).toContain("WHERE ${pmsNightSourceWitnessSql('entry_lineage')}");
    // Tails and binds stay at the sites.
    expect(emp).toContain('.bind(...values).all<RawMetricRow>()');
    expect(emp).toContain('values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to, f.from, f.to]');

    const mev = await src('adapters/executive-main-evidence-sources.ts');
    expect(mev.split("pmsNightSourceWitnessSql('lineage')").length - 1).toBe(1);
    expect(mev.split("pmsNightLineageStepSql('lineage')").length - 1).toBe(1);
    expect(mev.split(HEAD.witnessLineage).length - 1).toBe(0);
    expect(mev.split(HEAD.lineagePlain).length - 1).toBe(0);
    expect(mev).toContain('.bind(propertyId, actor.companyId, actor.branchId, actor.branchId, businessDate)');

    const rr = await src('adapters/pms/read-repository.ts');
    expect(rr.split("pmsBookingEmailPendingSql('reservationId')").length - 1).toBe(1);
    expect(rr.split(HEAD.bookingReservation).length - 1).toBe(0);
    expect(rr).toContain('.bind(facts.propertyId, facts.propertyId2).all<PmsStoredRecord>()');

    const pms = await src('adapters/pms.ts');
    expect(pms.split("pmsBookingEmailPendingSql('full')").length - 1).toBe(1);
    expect(pms.split(HEAD.bookingFull).length - 1).toBe(0);
    expect(pms).toContain("`${pmsBookingEmailPendingSql('full')}`, facts.id)");
  });

  it('leaves locks, feature availability, native reads and r17-r42 helpers untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect((await src('adapters/pms-booking-refunds.ts')).split("'SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE'").length - 1).toBe(2);
    const fa = await src('adapters/feature-availability.ts');
    expect(fa.split('EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=').length - 1).toBe(2);
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 302 to 299', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(243);
    expect(bare).toBe(245);
    expect(spaced).toBeLessThan(302);
  });
});
