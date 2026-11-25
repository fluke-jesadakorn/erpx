import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  pmsStayExistsHeadSql,
  PMS_CORRECTION_FACT_HEAD_SQL,
  PMS_ORDERED_RECORDS_TAIL_SQL,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r42-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r42-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: exact SQL text captured at HEAD before this dedupe. The SHA-256
// values pin the pre-change bytes; every closed-union recomposition must match
// byte-for-byte and keep the same bound parameters in the same order.
const HEAD = {
  stayRoom: "EXISTS(SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'roomId'=r.id",
  stayReservation: "EXISTS(SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'reservationId'=r.id",
  correctionFact: "SELECT 'correction',c.id,c.version,c.status FROM pms_charge_corrections c",
  orderedRecords: 'SELECT * FROM selected_records ORDER BY updated_at,id',
} as const;
const HEAD_SHA = {
  stayRoom: '0a7435331ed8ccaac4ccdef529e669444569d947001f8cfabe5227105cc82cf0',
  stayReservation: 'd5fa377b02e2859016ff4a7c4189646ed97022a82e4507278739b3e1bc621301',
  correctionFact: 'd64cec6604ee03358760c9627ac7d2a838daa4cb993d3dfb7baf7f0280338891',
  orderedRecords: '22c10829c267b13c399d7265753c5a460b000e37b677747ce6f59298aaad0ebf',
} as const;
const GENERATED = {
  stayRoom: pmsStayExistsHeadSql('roomId'),
  stayReservation: pmsStayExistsHeadSql('reservationId'),
  correctionFact: PMS_CORRECTION_FACT_HEAD_SQL,
  orderedRecords: PMS_ORDERED_RECORDS_TAIL_SQL,
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r42 stay-witness, correction-fact and order-tail dedupe', () => {
  it('pins the exact HEAD bytes of all four oracles', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
    }
  });

  it('declares each closed union exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split('pmsStayExistsHeadSql').length - 1).toBe(1);
    expect(shared.split('PMS_CORRECTION_FACT_HEAD_SQL').length - 1).toBe(1);
    expect(shared.split('PMS_ORDERED_RECORDS_TAIL_SQL').length - 1).toBe(1);
    expect(shared).toContain("type StayWitnessField = 'roomId' | 'reservationId'");
    // No caller SQL is accepted by any closed union.
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
  });

  it('generates all four byte-identical heads with unchanged bind counts', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(GENERATED[k], k).toBe(HEAD[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length).toBe((HEAD[k].match(/\?/g) ?? []).length);
    }
  });

  it('routes all call sites through the helpers and removes inline heads', async () => {
    const rr = await src('adapters/pms/read-repository.ts');
    // The roomId stay witness moved to native lane 20 with the room summary.
    expect(rr.split("pmsStayExistsHeadSql('roomId')").length - 1).toBe(0);
    expect(rr).toContain('roomSummaryFacts(db, actor, {');
    expect(rr.split("pmsStayExistsHeadSql('reservationId')").length - 1).toBe(1);
    expect(rr.split('${PMS_ORDERED_RECORDS_TAIL_SQL}').length - 1).toBe(1); // one after r48
    expect(rr.split(HEAD.stayRoom).length - 1).toBe(0);
    expect(rr.split(HEAD.stayReservation).length - 1).toBe(0);
    expect(rr.split(HEAD.orderedRecords).length - 1).toBe(0);
    // Tails and binds stay at the sites.
    expect(rr).not.toContain("AND st.data_json::jsonb->>'state'='in_house'))::integer occupied");
    expect(rr).toContain("AND ((st.data_json::jsonb->>'arrival'<=? AND st.data_json::jsonb->>'departure'>=?) OR st.data_json::jsonb->>'state'='in_house')) END)");
    expect(rr).not.toContain('.bind(facts.propertyId,facts.status,facts.status');
    expect(rr).toContain('.bind(facts.propertyId, facts.kind, facts.status, facts.status2');
    expect(rr).toContain('facts.fact24, facts.nightView3, facts.nightView4, facts.propertyId6], db');

    const addon = await src('adapters/addon-package-obligations.ts');
    expect(addon.split('${PMS_CORRECTION_FACT_HEAD_SQL}').length - 1).toBe(1);
    expect(addon.split(HEAD.correctionFact).length - 1).toBe(0);
    expect(addon).toContain('${PMS_CORRECTION_FACT_HEAD_SQL},scope WHERE c.company_id=scope.company_id');
    expect(addon).toContain('.bind(input.companyId,input.packageId).all()');

    const pkg = await src('adapters/package-obligations.ts');
    expect(pkg.split('${PMS_CORRECTION_FACT_HEAD_SQL}').length - 1).toBe(1);
    expect(pkg.split(HEAD.correctionFact).length - 1).toBe(0);
    expect(pkg).toContain("${PMS_CORRECTION_FACT_HEAD_SQL} JOIN properties p ON p.id=c.property_id WHERE c.status='pending'");
    expect(pkg).toContain('.bind(input.companyId).all()');
  });

  it('leaves locks, feature availability, native room board and r17-r41 fragments untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect((await src('adapters/pms-booking-refunds.ts')).split("'SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE'").length - 1).toBe(2);

    const fa = await src('adapters/feature-availability.ts');
    expect(fa.split('EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=').length - 1).toBe(2);
    expect(fa).toContain('f.company_id=${companyExpression}');
    expect(fa).toContain("f.module_id='${moduleId}'");
    expect(fa).toContain('activeRolloutPredicate(\'f\')');
    const shared = await src(SHARED);
    expect(shared).not.toContain('company_feature_rollouts');
    expect(shared).not.toContain('scheduledFeaturePredicate');

    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    const board = await src('adapters/room_board.rs');
    expect(board).toContain("EXISTS (SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'roomId'=r.id AND st.data_json::jsonb->>'state'='in_house'))::bigint occupied");
    expect(board.split("EXISTS(SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id").length - 1).toBe(0);
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41]) {
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
    expect(spaced).toBe(238);
    expect(bare).toBe(239);
    expect(spaced).toBeLessThan(302);
  });
});
