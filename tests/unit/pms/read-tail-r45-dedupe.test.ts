import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  pmsReservationRelationArmSql,
  pmsChosenKindPointSql,
  pmsRecordPropertyJoinSql,
  pmsRecordPropertyAnchorSql,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r45-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r45-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: exact SQL captured at HEAD (4a403bb7) before this dedupe.
// Every closed-union recomposition must match verbatim, with zero binds.
const HEAD = {
  armChosenReservation: "SELECT id FROM chosen WHERE kind='reservation'",
  armReservationIdChosen: "SELECT data->>'reservationId' FROM chosen",
  armSelectedGuestAccount: "SELECT id FROM selected WHERE kind='guest_account'",
  armChosenId: 'SELECT id FROM chosen',
  armStaysId: 'SELECT id FROM stays',
  armGuestAccountsId: 'SELECT id FROM guestAccounts',
  armReservationIdStays: "SELECT data->>'reservationId' FROM stays",
  armGuestIdStays: "SELECT data->>'guestId' FROM stays",
  armGuestIdChosen: "SELECT data->>'guestId' FROM chosen",
  pointGuestAccountStayAliased: "SELECT data->>'guestAccountId' id FROM stays",
  pointTypeIdStay: "SELECT data->>'typeId' FROM stays",
  pointRateIdStay: "SELECT data->>'rateId' FROM stays",
  pointGuestAccountSelected: "SELECT data->>'guestAccountId' FROM selected",
  joinOccupancyEvidence: "SELECT r.id,r.property_id,p.name property_name,p.config_json::jsonb->>'currency' property_currency,r.data_json::jsonb->>'date' business_date,\n      r.data_json::jsonb->>'state' state,r.data_json::jsonb->'roomMetrics'->>'capacity' capacity,\n      r.data_json::jsonb->'roomMetrics'->>'sold' sold,r.version\n    FROM pms_records r JOIN pms_properties p ON p.id=r.property_id",
  joinRoomCount: 'SELECT p.name label,count(*)::text value FROM pms_records r JOIN pms_properties p ON p.id=r.property_id',
  joinSourceIdentity: "SELECT r.id,r.property_id,r.kind,r.data_json::jsonb data,r.version,r.updated_at,\n    p.company_id,p.branch_id,p.name property_name,p.business_date\n  FROM pms_records r JOIN pms_properties p ON p.id=r.property_id",
  joinTransferList: "SELECT t.id,t.property_id,p.version property_version,p.config_json::jsonb->'accounts'->>'bank' bank_account,t.guest_account_id,t.state,t.amount_minor,t.currency,t.version,t.reference,t.recorded_by,\n      t.bank_transaction_id,t.entry_id,t.finance_payment_id,t.updated_at\n    FROM pms_transfer_requests t JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id",
  joinCrmAttachmentImage: "SELECT jsonb_build_object('record',to_jsonb(record),'property',to_jsonb(property))::text image FROM pms_records record JOIN pms_properties property ON property.id=record.property_id",
  joinCrmGuestSource: "SELECT guest.id,property.company_id,property.branch_id,property.id property_id FROM pms_records guest JOIN pms_properties property ON property.id=guest.property_id",
  anchorRP: 'FROM pms_records r JOIN pms_properties p ON p.id=r.property_id',
} as const;
const HEAD_SHA = {
  armChosenReservation: '5a666d2fbf6ebc0e433ad7f1af9bedaf153963c15e39dc6a9a28ece799898973',
  armReservationIdChosen: '1e9a33b048f8f512b3f911aed0df97a6faf8c8a1cce6111e9de1944185b945a1',
  armSelectedGuestAccount: '56e4046cadf06822c7feab07493614ff86d9b0d2f7b8624f83c9babb0955602b',
  armChosenId: '04938182ceefb707b4d4e29fb3353e011b31ab9407d4c558ca796f0736aa9b7a',
  armStaysId: '01e0c187d1046362fbfed1fddf62bef1998110184a8837f18553a749bd00ee2e',
  armGuestAccountsId: '817bae6939a7f9fa4da8e60564c1604fc0703e39720231f2009c2ba5bae6cba1',
  armReservationIdStays: 'c2456ef7a1fc6ff5f7615c193cfc1284a4eda1da9155a4d9a2cd67d0a0c92a53',
  armGuestIdStays: '7439cfbe394709e7a6076062c1ef1b78f1853a3e107c9ba0e051e562b7880756',
  armGuestIdChosen: '01cf470af14df88e79f8892044d98891807c976f54189acfd0157b15128a319f',
  pointGuestAccountStayAliased: '70d7d2a16846c9cba15c2b18160201019eb5646b8a8d8ffc012d9d3de2b73cb1',
  pointTypeIdStay: '1866e154f9d15b880ff6f7448d134634b06cee03cc4b94367b458f543427999d',
  pointRateIdStay: 'a9d3f64185c0e63da3b7fa3bf1622baba6d609eb170362e3fc5d9a49325e9c49',
  pointGuestAccountSelected: '8db823cd0d4fe0cb417c6817834e0190cc5277c72b6e38b259fec6b666370beb',
  joinOccupancyEvidence: '0b3fe3b9fd6367c961e36e45b55e5e8c3f60f551b621cb2891f29966420149ac',
  joinRoomCount: '96e7c56cf009d35fb49626be43ad41e2422fbb39e5f8aa36af65f825b803edc4',
  joinSourceIdentity: '502a18a3342fe9bdb64bf9085e11af032ad1162a3ab0b5a9bd39c46dba1309f5',
  joinTransferList: '49151231ec91e394b55cadca444f80c694d044d2932a69d30003a2630d1544dd',
  joinCrmAttachmentImage: 'f7eeae93c716d610bf5b46135906ade49cc77985c51bf76686e8ad81b015af09',
  joinCrmGuestSource: 'a84830470603d60e1e5bcbfe63c40b9bd2db095661d7bb510f3efe57abf13048',
  anchorRP: '6e202ee145038adbc17ffbf225257d38bee7b0843473960fb578a059afc555ef',
} as const;
const GENERATED = {
  armChosenReservation: pmsReservationRelationArmSql('chosenReservationId'),
  armReservationIdChosen: pmsReservationRelationArmSql('reservationIdChosen'),
  armSelectedGuestAccount: pmsReservationRelationArmSql('selectedGuestAccountId'),
  armChosenId: pmsReservationRelationArmSql('chosenId'),
  armStaysId: pmsReservationRelationArmSql('staysId'),
  armGuestAccountsId: pmsReservationRelationArmSql('guestAccountsId'),
  armReservationIdStays: pmsReservationRelationArmSql('reservationIdStays'),
  armGuestIdStays: pmsReservationRelationArmSql('guestIdStays'),
  armGuestIdChosen: pmsReservationRelationArmSql('guestIdChosen'),
  pointGuestAccountStayAliased: pmsChosenKindPointSql('guestAccountStayAliased'),
  pointTypeIdStay: pmsChosenKindPointSql('typeIdStay'),
  pointRateIdStay: pmsChosenKindPointSql('rateIdStay'),
  pointGuestAccountSelected: pmsChosenKindPointSql('guestAccountSelected'),
  joinOccupancyEvidence: pmsRecordPropertyJoinSql('occupancyEvidence'),
  joinRoomCount: pmsRecordPropertyJoinSql('roomCount'),
  joinSourceIdentity: pmsRecordPropertyJoinSql('sourceIdentity'),
  joinTransferList: pmsRecordPropertyJoinSql('transferList'),
  joinCrmAttachmentImage: pmsRecordPropertyJoinSql('crmAttachmentImage'),
  joinCrmGuestSource: pmsRecordPropertyJoinSql('crmGuestSource'),
  anchorRP: pmsRecordPropertyAnchorSql('r', 'p'),
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

describe('pms read-tail r45 relation-arm, point-head and join-anchor dedupe', () => {
  it('pins the exact HEAD bytes of all twenty oracles', () => {
    for (const k of KEYS) expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
  });

  it('declares each closed union exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(names(shared, 'pmsReservationRelationArmSql')).toBe(1);
    expect(names(shared, 'pmsChosenKindPointSql')).toBe(1);
    expect(names(shared, 'pmsRecordPropertyJoinSql')).toBe(1);
    expect(names(shared, 'pmsRecordPropertyAnchorSql')).toBe(1);
    expect(shared).toContain("type RelationArm =");
    expect(shared).toContain("type PointKind =");
    expect(shared).toContain("type JoinHead =");
    expect(shared).toContain('const RELATION_ARM');
    expect(shared).toContain('const POINT_HEAD');
    expect(shared).toContain('const OWNER_JOIN_HEAD');
    expect(shared).toContain('const CRM_JOIN_HEAD');
    // No caller SQL string is accepted by any closed union.
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
  });

  it('generates all twenty byte-identical heads with zero binds on the arms', () => {
    for (const k of KEYS) {
      expect(GENERATED[k], k).toBe(HEAD[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length, k).toBe(0);
    }
  });

  it('preserves arm order, optional point alias and the CRM join aliases', () => {
    expect(GENERATED.armChosenReservation).toContain("kind='reservation'");
    expect(GENERATED.armReservationIdChosen).toContain("data->>'reservationId' FROM chosen");
    expect(GENERATED.armSelectedGuestAccount).toContain("kind='guest_account'");
    expect(GENERATED.pointGuestAccountStayAliased).toContain("'guestAccountId' id FROM stays");
    expect(GENERATED.pointGuestAccountSelected).not.toContain(' id FROM');
    expect(GENERATED.pointTypeIdStay).toContain("'typeId' FROM stays");
    expect(GENERATED.pointRateIdStay).toContain("'rateId' FROM stays");
    expect(GENERATED.joinCrmAttachmentImage).toContain('JOIN pms_properties property ON property.id=record.property_id');
    expect(GENERATED.joinCrmGuestSource).toContain('JOIN pms_properties property ON property.id=guest.property_id');
    expect(GENERATED.joinTransferList).toContain('JOIN pms_properties p ON p.id=t.property_id AND p.company_id=t.company_id');
    expect(GENERATED.anchorRP).not.toContain('SELECT');
  });

  it('routes every call site through the helpers and removes the inline heads', async () => {
    const rr = await src('adapters/pms/read-repository.ts');
    expect(calls(rr, 'pmsReservationRelationArmSql')).toBe(4); // r48 native port
    expect(calls(rr, 'pmsChosenKindPointSql')).toBe(2); // r48 native port
    for (const k of [
      'armChosenReservation', 'armReservationIdChosen', 'armSelectedGuestAccount',
      'armReservationIdStays', 'armGuestIdStays', 'armGuestIdChosen',
      'pointGuestAccountStayAliased', 'pointTypeIdStay', 'pointRateIdStay', 'pointGuestAccountSelected',
    ] as const) expect(rr.split(HEAD[k]).length - 1, k).toBe(0);
    // Remaining arm order is preserved at the operation-related call site; the
    // section linked_ids union is now planned in Rust as one native kind.
    expect(rr).toContain("reservations AS (${pmsReservationRelationArmSql('chosenReservationId')} UNION ${pmsReservationRelationArmSql('reservationIdChosen')})");
    expect(rr).toContain('PMS_RD_SECTION_RELATED_RECORDS, [facts.propertyId, facts.seedsJson');
    // Binds and CTE/scoped tails stay at the call site.
    expect(rr).toContain('[facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3, facts.propertyId3, facts.roomView');
    expect(rr).toContain('.bind(facts.propertyId, facts.id, facts.propertyId2)');

    const mev = await src('adapters/executive-main-evidence-sources.ts');
    expect(calls(mev, 'pmsRecordPropertyJoinSql')).toBe(1);
    expect(mev.split(HEAD.joinOccupancyEvidence).length - 1).toBe(0);
    expect(mev).toContain('.bind(...values, recordId ? 1 : limit + 1)');

    const emp = await src('adapters/executive-metric-projections.ts');
    expect(calls(emp, 'pmsRecordPropertyJoinSql')).toBe(1);
    expect(calls(emp, 'pmsRecordPropertyAnchorSql')).toBe(1);
    expect(emp.split(HEAD.joinRoomCount).length - 1).toBe(0);
    expect(emp.split(HEAD.anchorRP).length - 1).toBe(0);
    expect(emp).toContain('values = [a.companyId, a.branchId, a.branchId, f.from, f.to]');

    for (const [name, kind] of [
      ['adapters/pms/source-query.ts', 'sourceIdentity'],
      ['adapters/pms/finance.ts', 'transferList'],
      ['adapters/crm-attachment-sources.ts', 'crmAttachmentImage'],
      ['adapters/crm-purchase-sources.ts', 'crmGuestSource'],
    ] as const) {
      const text = await src(name);
      expect(calls(text, 'pmsRecordPropertyJoinSql'), name).toBe(1);
      expect(text).toContain(`pmsRecordPropertyJoinSql('${kind}')`);
    }
  });

  it('pins the per-family call-site totals after r48', async () => {
    const shared = await src(SHARED);
    const r1 = shared.slice(shared.indexOf('/** Rank 1'), shared.indexOf('/** Rank 2'));
    const r2 = shared.slice(shared.indexOf('/** Rank 2'), shared.indexOf('/** Rank 3'));
    const r3 = shared.slice(shared.indexOf('/** Rank 3'));
    expect(r1.split('SELECT ').length - 1).toBe(7);
    expect(r2.split('SELECT ').length - 1).toBe(2);
    expect(r3.split('SELECT ').length - 1).toBe(5);
    let a = 0, p = 0, j = 0;
    for (const file of await tsFiles(dir)) {
      if (file.endsWith(SHARED)) continue;
      const text = await readFile(file, 'utf8');
      a += calls(text, 'pmsReservationRelationArmSql');
      p += calls(text, 'pmsChosenKindPointSql');
      j += calls(text, 'pmsRecordPropertyJoinSql');
    }
    expect([a, p, j]).toEqual([4, 2, 6]);
  });

  it('leaves locks, feature availability, native reads and r17-r44 helpers untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect((await src('adapters/pms-booking-refunds.ts')).split("'SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE'").length - 1).toBe(2);
    const fa = await src('adapters/feature-availability.ts');
    expect(fa.split('EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=').length - 1).toBe(2);
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44]) {
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
    expect(spaced).toBe(216);
    expect(bare).toBe(217);
    expect(spaced).toBeLessThan(292);
    expect(bare).toBeLessThan(294);
  });
});
