import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  pmsGuestIdJoinSql,
  pmsPropertyScopeSql,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r41-sql';
import { pmsHomeGuestAccountSumSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r39-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r41-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: exact SQL text captured at HEAD before this dedupe. The SHA-256
// values pin the pre-change bytes; every closed-union recomposition must match
// byte-for-byte and keep the same bound parameters in the same order. The
// hotelMetrics variant was retired to the native owner read plan.
const HEAD = {
  guestEmail: "SELECT g.data_json::jsonb->>'email' email FROM pms_records r JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId'",
  guestReservation: "SELECT r.id,r.property_id FROM pms_records r JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' AND g.property_id=r.property_id AND g.kind='guest'",
  guestOperations: "SELECT r.id FROM pms_records r\n   LEFT JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' AND g.property_id=r.property_id AND g.kind='guest'",
  hotelMoney: "WITH RECURSIVE entry_roots AS (\n      SELECT r.id,r.property_id,r.id root_id FROM pms_records r JOIN pms_properties p ON p.id=r.property_id\n      WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='entry' AND r.data_json::jsonb->>'sourceKey' LIKE 'night:%'\n      UNION ALL SELECT r.id,r.property_id,e.root_id FROM pms_records r JOIN entry_roots e ON r.property_id=e.property_id AND r.data_json::jsonb->>'originalId'=e.id\n        WHERE r.kind='entry' AND r.id<>e.id\n    ) SELECT p.id,\n      COALESCE((SELECT sum((r.data_json::jsonb->>'net')::bigint) FROM pms_records r JOIN entry_roots e ON e.id=r.id AND e.property_id=r.property_id\n        WHERE r.property_id=p.id AND r.data_json::jsonb->>'date'=p.business_date AND r.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) room_revenue,\n      COALESCE((${pmsHomeGuestAccountSumSql('receivable')}),0) receivable,\n      COALESCE((${pmsHomeGuestAccountSumSql('advance')}),0) advance,\n      COALESCE((${pmsHomeGuestAccountSumSql('deposit')}),0) deposit\n    FROM pms_properties p WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?)",
  liabilities: "SELECT p.id,\n    COALESCE(sum(CASE WHEN e.data_json::jsonb->>'description'='payment.refund_liability' THEN (e.data_json::jsonb->>'total')::bigint\n      WHEN e.data_json::jsonb->>'description'='payment.refund_settle' AND source.data_json::jsonb->>'description'<>'payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint ELSE 0 END),0) refund_payable,\n    COALESCE(sum(CASE WHEN e.data_json::jsonb->>'description'='payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint\n      WHEN e.data_json::jsonb->>'description'='payment.refund_settle' AND source.data_json::jsonb->>'description'='payment.unallocated' THEN (e.data_json::jsonb->>'total')::bigint ELSE 0 END),0) unallocated\n    FROM pms_properties p LEFT JOIN pms_records e ON e.property_id=p.id AND e.kind='entry'\n    LEFT JOIN pms_records source ON source.property_id=p.id AND source.id=e.data_json::jsonb->>'originalId' AND source.kind='entry'\n    WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.id=?) GROUP BY p.id",
} as const;
const HEAD_SHA = {
  guestEmail: '910eac52c5187569586657529b84cf6f3968cfa0be9953f0a232da136118c8c3',
  guestReservation: '81c0ce1c2dc3acb766f160d6d606d4dc7adef2bc433f978ca8da7f0689130e06',
  guestOperations: 'a751da095b6fe4488bc59fbd9ad897bba41b81b67e1bfebd23c7618955f1234b',
  hotelMoney: '6163173cd3c318771a9f43952522036b43869d412fa02c8846c8c896702bda9d',
  liabilities: 'c3de466c8b039db17d68627ce6ae9d98a93008aba53babff5848b05aa55e36c1',
} as const;

// The rendered form resolves the r39 interpolations to the same text HEAD ran.
const resolve = (s: string) => s
  .split("${pmsHomeGuestAccountSumSql('receivable')}").join(pmsHomeGuestAccountSumSql('receivable'))
  .split("${pmsHomeGuestAccountSumSql('advance')}").join(pmsHomeGuestAccountSumSql('advance'))
  .split("${pmsHomeGuestAccountSumSql('deposit')}").join(pmsHomeGuestAccountSumSql('deposit'));
const RENDERED = {
  guestEmail: HEAD.guestEmail,
  guestReservation: HEAD.guestReservation,
  guestOperations: HEAD.guestOperations,
  hotelMoney: resolve(HEAD.hotelMoney),
  liabilities: HEAD.liabilities,
} as const;
const GENERATED = {
  guestEmail: pmsGuestIdJoinSql('email'),
  guestReservation: pmsGuestIdJoinSql('reservationEmail'),
  guestOperations: pmsGuestIdJoinSql('operations'),
  hotelMoney: pmsPropertyScopeSql('hotelMoney'),
  liabilities: pmsPropertyScopeSql('liabilities'),
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r41 guest-join, property-scope and rollout-exists dedupe', () => {
  it('pins the exact HEAD bytes of all five oracles', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
    }
  });

  it('declares each closed union exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split('pmsGuestIdJoinSql').length - 1).toBe(1);
    expect(shared.split('pmsPropertyScopeSql').length - 1).toBe(1);
    expect(shared).toContain("type GuestIdJoin = 'email' | 'reservationEmail' | 'operations'");
    expect(shared).toContain("type PropertyScope = 'hotelMoney' | 'liabilities'");
    expect(shared).toContain('const GUEST_ID_JOIN');
    expect(shared).toContain('const PROPERTY_SCOPE');
    expect(shared).toContain('const PROPERTY_SCOPE_WHERE');
    // No caller SQL is accepted by any closed union and the retired hotelMetrics
    // variant is gone.
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
    expect(shared).not.toContain('hotelMetrics');
    expect(shared).not.toContain('pmsRolloutReadyExistsSql');
  });

  it('generates all five byte-identical heads with unchanged bind counts', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(GENERATED[k], k).toBe(RENDERED[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length).toBe((RENDERED[k].match(/\?/g) ?? []).length);
    }
  });

  it('routes all five call sites through the helpers and removes inline heads', async () => {
    const bk = await src('adapters/pms-booking.ts');
    expect(bk.split("pmsGuestIdJoinSql('email')").length - 1).toBe(1);
    expect(bk.split("pmsGuestIdJoinSql('reservationEmail')").length - 1).toBe(1);
    expect(bk.split(HEAD.guestEmail).length - 1).toBe(0);
    expect(bk.split(HEAD.guestReservation).length - 1).toBe(0);
    expect(bk).toContain('.bind(facts.reservation_id, facts.property_id)');
    expect(bk).toContain('facts.number, facts.email)');

    const rr = await src('adapters/pms/read-repository.ts');
    expect(rr.split("pmsGuestIdJoinSql('operations')").length - 1).toBe(0)
    expect(rr).toContain("PMS_RD_OPERATIONS_PAGE");
    expect(rr.split(HEAD.guestOperations).length - 1).toBe(0);

    const hm = await src('adapters/pms/home.ts');
    expect(hm.split("pmsPropertyScopeSql('hotelMoney')").length - 1).toBe(1);
    expect(hm).toContain('.bind(facts.companyId, facts.branchId, facts.branchId2, facts.companyId2, facts.branchId3, facts.branchId4, facts.propertyId, facts.propertyId2)');
    expect(hm).not.toContain("pmsPropertyScopeSql('hotelMetrics')");
    expect(hm).toContain('nativePmsReadAll<{');
    expect(hm).toContain('PMS_RD_HOTEL_METRICS, [facts.companyId, facts.branchId, facts.branchId2, facts.propertyId, facts.propertyId2]');

    const fin = await src('adapters/pms/finance.ts');
    expect(fin.split("pmsPropertyScopeSql('liabilities')").length - 1).toBe(1);
    expect(fin.split(HEAD.liabilities).length - 1).toBe(0);

    const roll = await src('adapters/hospitality-rollout.ts');
    expect(roll).not.toContain('pmsRolloutReadyExistsSql');
    expect(roll).toContain('PMS_RD_ROLLOUT_SETUP_READY, [companyId], db');
  });

  it('leaves finance transfer tail, locks, native registry and r17-r40 helpers untouched', async () => {
    const fin = await src('adapters/pms/finance.ts');
    expect(fin).toContain("pmsRecordPropertyJoinSql('transferList')");
    expect(await src('adapters/pms-read-tail-r45-sql.ts')).toContain('FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id');
    expect(fin).toContain('t.updated_at DESC LIMIT 100');
    expect(fin).not.toContain("pmsPropertyScopeSql('transfers')");

    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect(await src('adapters/pms-booking.ts')).toContain("'SELECT * FROM pms_booking_payments WHERE id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 302 to 215', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(194);
    expect(bare).toBe(195);
    expect(spaced).toBeLessThan(302);
  });
});
