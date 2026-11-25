import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { describe, expect, it } from 'vitest';
import { hospitalityOwnerProjectionSources } from '@erpx/server-organization/public/hospitality-owner-projection-sources';
import {
  pmsBookingReplaySql,
  pmsBranchLabelSql,
  PMS_REFUND_NON_LOCK_HEAD_SQL,
} from '../../../packages/server-pms/src/adapters/pms-read-tail-r40-sql';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r40-sql.ts';
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

const ORG = hospitalityOwnerProjectionSources.organization_nodes;
const PH = '${organizationProjectionSources.organization_nodes}';

// Byte oracles: exact SQL text as it appeared at HEAD before this dedupe. The
// SHA-256 values pin the pre-change bytes; every recomposition must match the
// HEAD text byte-for-byte and keep the same bound parameters in the same order.
const HEAD = {
  receipts: 'SELECT 1 FROM pms_booking_receipts WHERE id=? AND property_id=?',
  changes: 'SELECT 1 FROM pms_booking_changes WHERE id=? AND property_id=?',
  branchN: `COALESCE((SELECT n.display_name FROM (${PH}) n WHERE n.id=p.branch_id AND n.company_id=p.company_id),'Unassigned branch')`,
  branchN2: `COALESCE((SELECT n2.display_name FROM (${PH}) n2 WHERE n2.id=p.branch_id AND n2.company_id=p.company_id),'Unassigned branch')`,
  refundById: 'SELECT * FROM pms_booking_refund_requests WHERE id=?',
  refundByState: "SELECT * FROM pms_booking_refund_requests WHERE (id=? OR payment_id=?) AND state IN ('approved','processing','requires_action') ORDER BY created_at",
} as const;
const HEAD_SHA = {
  receipts: '0a48895768e21aa73477658732597fbb9af486243b290b02ac8d2ab15cfc4449',
  changes: '8ee8e77dc13b1289dbd8f688e6445a442b842ad1cdd43a7e9ed35a2e9fa1eb6a',
  branchN: '45c661c71d6791a9e10b70bb3f99f84f189cc69e810129a9da2e4a6c76c7ac19',
  branchN2: '57a237aa21672d8b09185bcaf25b419e7e97e71ff76a0a3b0b211a9ba0f07c13',
  refundById: '6585022c6b6a418e9b09da7e6b8a8e0c3dddd702d8606abe39c96fe6fba93334',
  refundByState: '921bd112fe757ccf709185c6125646d9d6b5d5e05c52b843993eaa4b2ef2a3b7',
} as const;

// The rendered form is the HEAD text with the org-nodes interpolation resolved.
const RENDERED = {
  receipts: HEAD.receipts,
  changes: HEAD.changes,
  branchN: HEAD.branchN.split(PH).join(ORG),
  branchN2: HEAD.branchN2.split(PH).join(ORG),
  refundById: HEAD.refundById,
  refundByState: HEAD.refundByState,
} as const;

const GENERATED = {
  receipts: pmsBookingReplaySql('receipt'),
  changes: pmsBookingReplaySql('change'),
  branchN: pmsBranchLabelSql('n'),
  branchN2: pmsBranchLabelSql('n2'),
  refundById: `${PMS_REFUND_NON_LOCK_HEAD_SQL}id=?`,
  refundByState: `${PMS_REFUND_NON_LOCK_HEAD_SQL}(id=? OR payment_id=?) AND state IN ('approved','processing','requires_action') ORDER BY created_at`,
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r40 receipt/change, branch-label and refund dedupe', () => {
  it('pins the exact HEAD bytes of all six oracles', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(sha256(HEAD[k]), k).toBe(HEAD_SHA[k]);
    }
  });

  it('confirms each oracle is the exact text replaced at its HEAD call site', () => {
    const atHead = (p: string) => execFileSync('git', ['show', `5d67645ce:${p}`], { encoding: 'utf8' });
    expect(atHead('packages/server-pms/src/adapters/property-purchase-boundary.ts')).toContain(HEAD.receipts);
    expect(atHead('packages/server-pms/src/adapters/property-purchase-boundary.ts')).toContain(HEAD.changes);
    const emHead = atHead('packages/server-pms/src/adapters/executive-metric-projections.ts');
    expect(emHead).toContain(HEAD.branchN);
    expect(emHead).toContain(HEAD.branchN2);
    const rfHead = atHead('packages/server-pms/src/adapters/pms-booking-refunds.ts');
    expect(rfHead).toContain(HEAD.refundById);
    expect(rfHead).toContain(HEAD.refundByState);
  });

  it('declares each closed union exactly once in the shared module', async () => {
    const shared = await src(SHARED);
    expect(shared.split('pmsBookingReplaySql').length - 1).toBe(1);
    expect(shared.split('pmsBranchLabelSql').length - 1).toBe(1);
    expect(shared.split('PMS_REFUND_NON_LOCK_HEAD_SQL').length - 1).toBe(1);
    expect(shared).toContain("const PMS_BOOKING_REPLAY_TABLE");
    expect(shared).toContain("type PmsBranchLabelAlias = 'n' | 'n2'");
    // No caller SQL is accepted by any closed union.
    expect(shared).not.toMatch(/\(\s*\w+\s*:\s*string\s*\)\s*=>/);
  });

  it('generates all six byte-identical heads with unchanged bind counts', () => {
    for (const k of Object.keys(HEAD) as (keyof typeof HEAD)[]) {
      expect(GENERATED[k], k).toBe(RENDERED[k]);
      expect((GENERATED[k].match(/\?/g) ?? []).length).toBe((RENDERED[k].match(/\?/g) ?? []).length);
    }
  });

  it('resolves the branch-label interpolation without changing any other byte', () => {
    expect(RENDERED.branchN).toBe(HEAD.branchN.split(PH).join(ORG));
    expect(RENDERED.branchN2).toBe(HEAD.branchN2.split(PH).join(ORG));
    expect(RENDERED.branchN.startsWith('COALESCE((SELECT n.display_name')).toBe(true);
    expect(RENDERED.branchN2.startsWith('COALESCE((SELECT n2.display_name')).toBe(true);
  });

  it('routes all six call sites through the shared heads and removes inline heads', async () => {
    const pp = await src('adapters/property-purchase-boundary.ts');
    expect(pp.split('pmsBookingReplaySql("receipt")').length - 1).toBe(1);
    expect(pp.split('pmsBookingReplaySql("change")').length - 1).toBe(1);
    expect(pp.split(HEAD.receipts).length - 1).toBe(0);
    expect(pp.split(HEAD.changes).length - 1).toBe(0);

    const em = await src('adapters/executive-metric-projections.ts');
    expect(em.split("pmsBranchLabelSql('n')").length - 1).toBe(1);
    expect(em.split("pmsBranchLabelSql('n2')").length - 1).toBe(1);
    expect(em.split('COALESCE((SELECT').length - 1).toBe(0);
    expect(em.split(HEAD.branchN).length - 1).toBe(0);
    expect(em.split(HEAD.branchN2).length - 1).toBe(0);

    const rf = await src('adapters/pms-booking-refunds.ts');
    expect(rf.split('${PMS_REFUND_NON_LOCK_HEAD_SQL}').length - 1).toBe(2);
    // The two row-lock reads keep their own literal; no non-lock inline head remains.
    expect(rf.split("SELECT * FROM pms_booking_refund_requests WHERE id=?'").length - 1).toBe(0);
    expect(rf.split(HEAD.refundByState).length - 1).toBe(0);
  });

  it('keeps locks, the native read registry and the r17-r39 fragments untouched', async () => {
    expect(await src(SHARED)).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|FOR\s+KEY\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const stripe = await src('adapters/pms-stripe.ts');
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE'");
    expect(stripe).toContain("'SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE'");
    const refunds = await src('adapters/pms-booking-refunds.ts');
    expect(refunds.split("'SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE'").length - 1).toBe(2);
    expect(refunds).toContain("'SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE'");
    expect(await src('adapters/native-pms-read.ts')).toContain('PMS_RD_CHARGE_CORRECTION_PENDING_COUNT');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length).toBeGreaterThan(0);
    }
  });

  it('skips rank 5 (feature availability): a closed union would have to accept caller SQL', async () => {
    const fa = await src('adapters/feature-availability.ts');
    // The two EXISTS witnesses still interpolate a caller SQL expression, a
    // caller module literal and a caller predicate alias; no enum closes them.
    expect(fa.split('EXISTS(SELECT 1 FROM company_feature_rollouts f WHERE f.company_id=').length - 1).toBe(2);
    expect(fa).toContain('f.company_id=${companyExpression}');
    expect(fa).toContain("f.module_id='${moduleId}'");
    expect(fa).toContain("f.company_id=engineering_subject.company_id");
    expect(fa).toContain("activeRolloutPredicate('f')");
    // r40 introduces no feature-availability head or caller-SQL helper.
    const shared = await src(SHARED);
    expect(shared).not.toContain('company_feature_rollouts');
    expect(shared).not.toContain('scheduledFeaturePredicate');
  });

  it('lowers the pms owner SELECT KPI from 313 to 310', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(234);
    expect(bare).toBe(235);
    expect(spaced).toBeLessThan(313);
  });
});
