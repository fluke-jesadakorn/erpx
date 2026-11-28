import { readFile } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import {
  BILLING_ROLLOUT_ELIGIBILITY_SQL,
  BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL,
  BILLING_MONTH_INVOICE_SQL,
} from '../../../modules/billing/server/src/adapters/billing-read-tail-r16-sql';

const GROUPS: [string, string, string][] = [
  ['BILLING_ROLLOUT_ELIGIBILITY_SQL', BILLING_ROLLOUT_ELIGIBILITY_SQL, `SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1
     UNION ALL SELECT 1 FROM billing_target_rollout_approvals
     WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1`],
  ['BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL', BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL, `SELECT 1 FROM billing_enrollments WHERE holding_id=$1 AND plan_id<>'platform_usage'
     AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2`],
  ['BILLING_MONTH_INVOICE_SQL', BILLING_MONTH_INVOICE_SQL, `SELECT 1 FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id
     WHERE i.holding_id=$1 AND p.month=$2 AND i.status<>'void'`],
];
const SUBSCRIPTION_SELECT = 'SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions';

describe('billing read-tail r16 byte-identical SQL dedupe', () => {
  it('keeps exactly one literal per family in the shared constants file', async () => {
    const shared = await readFile(
      new URL('../../../modules/billing/server/src/adapters/billing-read-tail-r16-sql.ts', import.meta.url),
      'utf8',
    );
    expect(shared.split('SELECT ').length - 1).toBe(0); // r36 routes the UNION ALL guard through r29
    for (const [name, actual, literal] of GROUPS) {
      expect(actual).toBe(literal);
      expect(shared.split(literal).length - 1).toBe(0);
      expect(shared).toContain(name);
    }
  });

  it('removes every inline family literal from both owner-local repositories', async () => {
    for (const file of ['billing.ts', 'billing-target.ts']) {
      const text = await readFile(
        new URL(`../../../modules/billing/server/src/adapters/${file}`, import.meta.url),
        'utf8',
      );
      expect(text.split("'./billing-read-tail-r16-sql'").length - 1).toBe(1);
      for (const [name, , literal] of GROUPS) {
        expect(text.split(literal).length - 1).toBe(0);
        expect(text.split(name).length - 1).toBe(2);
      }
    }
  });

  it('shares the byte-identical commercial subscription projection prefix', async () => {
    const text = await readFile(
      new URL('../../../modules/billing/server/src/adapters/executive-main-evidence-sources.ts', import.meta.url),
      'utf8',
    );
    // Native lane 38 wave 2: the owner crate owns this statement text.
    expect(text.split(SUBSCRIPTION_SELECT).length - 1).toBe(0);
    expect(text).not.toContain('billingProjectionSql(');
    expect(text).not.toContain('commercialSubscriptionSelect()');
    expect(text).toContain("from './native-lane-38/client'");
    expect(text).toContain('nativeBillingCommercialSubscriptions(');
  });
});
