import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import {
  pmsChosenKindUnionSql,
  pmsPaymentUnionSql,
  pmsNightLineageAnchorSql,
} from '../../../modules/pms/server/src/adapters/pms-read-tail-r44-sql';

const dir = new URL('../../../modules/pms/server/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r44-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles: exact SQL captured at HEAD before this dedupe, including the
// two r44 anchor indents. Every closed-union recomposition must match verbatim.
const HEAD = {
  chosenGuest: "SELECT id FROM chosen WHERE kind='guest_account' UNION SELECT data->>'guestAccountId' FROM stays UNION SELECT data->>'guestAccountId' FROM chosen",
  chosenRoom: "SELECT id FROM chosen WHERE kind='room' UNION SELECT data->>'roomId' FROM stays UNION SELECT data->>'roomId' FROM chosen",
  identityTrue: "SELECT p.company_id,d.finance_id id FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL UNION SELECT company_id,finance_payment_id FROM pms_transfer_requests WHERE finance_payment_id IS NOT NULL UNION SELECT p.company_id,b.finance_payment_id FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL",
  identityFalse: "SELECT p.company_id,d.finance_id FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL UNION SELECT company_id,finance_payment_id FROM pms_transfer_requests WHERE finance_payment_id IS NOT NULL UNION SELECT p.company_id,b.finance_payment_id FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL",
  sourceImage: "SELECT p.company_id,d.finance_id id,jsonb_build_object('property',to_jsonb(p),'document',to_jsonb(d)) source_image FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL UNION ALL SELECT company_id,finance_payment_id,to_jsonb(d) FROM pms_transfer_requests d WHERE finance_payment_id IS NOT NULL UNION ALL SELECT p.company_id,b.finance_payment_id,jsonb_build_object('property',to_jsonb(p),'balance',to_jsonb(b)) FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL",
  anchorEntryFull: '        SELECT id,property_id,business_date,net_raw,id,source_key,original_id,ARRAY[id]::text[] FROM candidate_entries',
  anchorPlainFull: '      SELECT id,property_id,net_raw,id,source_key,original_id,ARRAY[id]::text[] FROM candidate_entries',
} as const;
const HEAD_SHA = {
  chosenGuest: 'a618bf3b25ac7005fb08c0d161be906c8198854177bf4d8dbde22ad2a10a70bd',
  chosenRoom: '13d65dc73de5caf3d8df7a5408a90409f06a06d656953ffebc203b794738b373',
  identityTrue: 'edb55539355ffd12bf64586a86304e8425a03b83597a38ad738cc68d9ef7c4bc',
  identityFalse: '086927f6f07bad2f95889a953daddc9bfd8bdc6e0eaa08f7ed10bbc09de07575',
  sourceImage: '7caab7f88cca75ad120f24b3ae75e940fef3ef66a479b4b1593479007c002838',
  anchorEntryFull: '4c7ba887a6846a3dac35e8c2cac6760dc0033d08554bea58ff2fd5d64086d915',
  anchorPlainFull: 'fdd599bd881b59a0c0a3e1a5bb238222c700547490d9a39385b81b8c2aca723b',
} as const;
const GENERATED = {
  chosenGuest: pmsChosenKindUnionSql('guest_account'),
  chosenRoom: pmsChosenKindUnionSql('room'),
  identityTrue: pmsPaymentUnionSql('identity', true),
  identityFalse: pmsPaymentUnionSql('identity', false),
  sourceImage: pmsPaymentUnionSql('sourceImage'),
  anchorEntryFull: pmsNightLineageAnchorSql('entry_lineage'),
  anchorPlainFull: pmsNightLineageAnchorSql('lineage'),
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r44 chosen-kind, payment-union and lineage-anchor dedupe', () => {
  it('pins the exact HEAD bytes of all seven oracles', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
    }
  });

  it('declares each closed union exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split('pmsChosenKindUnionSql').length - 1).toBe(1);
    expect(shared.split('pmsPaymentUnionSql').length - 1).toBe(1);
    expect(shared.split('pmsNightLineageAnchorSql').length - 1).toBe(1);
    expect(shared).toContain("type ChosenKind = 'guest_account' | 'room'");
    expect(shared).toContain("type PaymentUnionKind = 'identity' | 'sourceImage'");
    expect(shared).toContain("type LineageAnchorKind = 'entry_lineage' | 'lineage'");
    expect(shared).toContain('const CHOSEN_KIND_ID_FIELD');
    expect(shared).toContain('const PAYMENT_UNION');
    expect(shared).toContain('const PAYMENT_TRANSFER_ALIAS');
    expect(shared).toContain('const NIGHT_LINEAGE_ANCHOR');
    // No caller SQL is accepted by any closed union.
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
  });

  it('generates all seven byte-identical heads with unchanged zero binds', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(GENERATED[k], k).toBe(HEAD[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length).toBe(0);
    }
  });

  it('preserves UNION vs UNION ALL, the document alias flag and the image extras', () => {
    expect(GENERATED.identityTrue).toContain("d.finance_id id FROM pms_documents");
    expect(GENERATED.identityFalse).toContain('d.finance_id FROM pms_documents');
    expect(GENERATED.identityTrue.split(' UNION SELECT ').length - 1).toBe(2);
    expect(GENERATED.identityTrue).not.toContain('UNION ALL');
    expect(GENERATED.sourceImage.split(' UNION ALL SELECT ').length - 1).toBe(2);
    expect(GENERATED.sourceImage).toContain("'document',to_jsonb(d)) source_image");
    expect(GENERATED.sourceImage).toContain('finance_payment_id,to_jsonb(d) FROM pms_transfer_requests d');
    expect(GENERATED.sourceImage).toContain("'balance',to_jsonb(b))");
    expect(GENERATED.sourceImage).not.toContain(' UNION SELECT ');
    expect(GENERATED.chosenGuest).toContain("kind='guest_account' UNION SELECT data->>'guestAccountId' FROM stays");
    expect(GENERATED.chosenRoom).toContain("kind='room' UNION SELECT data->>'roomId' FROM stays");
    expect(GENERATED.anchorEntryFull).toContain('business_date');
    expect(GENERATED.anchorPlainFull).not.toContain('business_date');
  });

  it('routes all call sites through the helpers and removes the inline heads', async () => {
    const rr = await src('adapters/pms/read-repository.ts');
    expect(rr.split("pmsChosenKindUnionSql('guest_account')").length - 1).toBe(0); // r48 native port
    expect(rr.split("pmsChosenKindUnionSql('room')").length - 1).toBe(0); // r48 native port
    for (const k of ['chosenGuest', 'chosenRoom'] as const) expect(rr.split(HEAD[k]).length - 1).toBe(0);
    expect(rr).not.toContain("guestAccounts AS (${pmsChosenKindUnionSql('guest_account')})");
    expect(rr).not.toContain("room_ids AS (${pmsChosenKindUnionSql('room')})");
    // Binds and CTE aliases stay at the site.
    expect(rr).toContain('PMS_RD_OPERATION_RELATED_RECORDS, [facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3, facts.propertyId3], db');

    const fin = await src('adapters/finance-record-sources.ts');
    expect(fin.split("pmsPaymentUnionSql('sourceImage')").length - 1).toBe(1);
    for (const k of ['identityTrue', 'identityFalse', 'sourceImage'] as const) expect(fin.split(HEAD[k]).length - 1).toBe(0);
    expect(fin).toContain('export function pmsPaymentIdentityUnionSql(aliasDocumentId: boolean): string');
    expect(fin).toContain("return pmsPaymentUnionSql('identity', aliasDocumentId);");
    expect(fin).toContain('payment: pmsPaymentIdentityUnionSql(true)');
    expect(fin).toContain('LOCK TABLE pms_properties,pms_documents,pms_postings,pms_import_batches,pms_transfer_requests,pms_import_balances IN SHARE ROW EXCLUSIVE MODE');
    const pkg = await src('adapters/package-obligations.ts');
    expect(pkg).toContain('import { pmsPaymentIdentityUnionSql } from "./finance-record-sources";');
    expect(pkg).toContain('payments: pmsPaymentIdentityUnionSql(false)');

    const emp = await src('adapters/executive-metric-projections.ts');
    expect(emp.split("pmsNightLineageAnchorSql('entry_lineage')").length - 1).toBe(1);
    expect(emp.split(HEAD.anchorEntryFull).length - 1).toBe(0);
    expect(emp).toContain("WHERE ${pmsNightSourceWitnessSql('entry_lineage')}");
    expect(emp).toContain('.bind(...values).all<RawMetricRow>()');
    expect(emp).toContain('values = [a.companyId, a.branchId, a.branchId, f.currency, f.from, f.to, f.from, f.to]');

    const mev = await src('adapters/executive-main-evidence-sources.ts');
    expect(mev.split("pmsNightLineageAnchorSql('lineage')").length - 1).toBe(1);
    expect(mev.split(HEAD.anchorPlainFull).length - 1).toBe(0);
    expect(mev).toContain('.bind(propertyId, actor.companyId, actor.branchId, actor.branchId, businessDate)');
  });

  it('leaves locks, feature availability, native reads and r17-r43 helpers untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    expect((await src('adapters/pms-booking-refunds.ts')).split("'SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE'").length - 1).toBe(2);
    const fa = await src('adapters/feature-availability.ts');
    expect(fa.split('EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=').length - 1).toBe(2);
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43]) {
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
    expect(spaced).toBe(194);
    expect(bare).toBe(195);
    expect(spaced).toBeLessThan(299);
    expect(bare).toBeLessThan(301);
  });
});
