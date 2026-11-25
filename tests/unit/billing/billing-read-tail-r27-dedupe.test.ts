import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { BILLING_PLAN_NAME_LATERAL_SQL } from '../../../packages/server-billing/src/adapters/billing-read-tail-r18-sql';
import { billingMetricCountSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r27-sql';

const dir = new URL('../../../packages/server-billing/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');

/** Verbatim pre-r27 parameterized count projection. Byte-identity oracle only. */
const metricCount = (label: string) => `SELECT ${label} label,count(*)::text value FROM `;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r27 metric count projection dedupe', () => {
  it('authors the parameterized count projection once and renders every label byte-identically', async () => {
    const helper = await src('billing-read-tail-r27-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(0); // r35 generic projection head owns the SELECT
    expect(helper).toContain("from './billing-read-tail-r35-sql'");
    for (const label of ['status', 'plan', "'Active subscriptions'", "'Overdue subscription invoices'", 'i.status']) {
      expect(billingMetricCountSql(label)).toBe(metricCount(label));
    }
    expect(billingMetricCountSql('status')).toBe('SELECT status label,count(*)::text value FROM ');
  });

  it('reproduces both former inline statements byte for byte', () => {
    const label = "'Active subscriptions'";
    const activeSubscriptions = `WITH subscriptions AS (
        SELECT e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status
        FROM billing_enrollments e
        ${BILLING_PLAN_NAME_LATERAL_SQL}
        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL
          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
        UNION ALL
        SELECT b.holding_id id,'Legacy subscription' plan,b.status
        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)
      ) ${billingMetricCountSql(label)}subscriptions GROUP BY 1 ORDER BY 1`;
    expect(activeSubscriptions).toBe(`WITH subscriptions AS (
        SELECT e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status
        FROM billing_enrollments e
        ${BILLING_PLAN_NAME_LATERAL_SQL}
        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL
          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)
        UNION ALL
        SELECT b.holding_id id,'Legacy subscription' plan,b.status
        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)
      ) SELECT ${label} label,count(*)::text value FROM subscriptions GROUP BY 1 ORDER BY 1`);

    const overdueInvoices = `${billingMetricCountSql(label)}billing_invoices i
      WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?
      GROUP BY 1 ORDER BY 1`;
    expect(overdueInvoices).toBe(`SELECT ${label} label,count(*)::text value FROM billing_invoices i
      WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?
      GROUP BY 1 ORDER BY 1`);
  });

  it('routes both metric projections through the shared text with their own binds', async () => {
    const text = await src('executive-metric-projections.ts');
    expect(text.split("import { billingMetricCountSql } from './billing-read-tail-r27-sql';").length - 1).toBe(1);
    expect(text.split('billingMetricCountSql(label)').length - 1).toBe(2);
    expect(text).not.toContain('SELECT ${label} label,count(*)::text value FROM ');
    expect(text).toContain("from './billing-read-tail-r33-sql'");
    expect(text).toContain('billingEnrollmentSubscriptionSql(');
    expect(text).toContain('values = [a.holdingId, now, now, a.holdingId, now];');
    expect(text).toContain('values = [a.holdingId, Date.parse(f.from), Date.parse(f.to) + 86400000, Date.now()];');
    // CTE row heads, the plan-name lateral, watermark and evidence SQL stay untouched.
    expect(text.split('e.id,COALESCE(p.name,e.plan_id) plan').length - 1).toBe(1);
    expect(text.split('billingLegacySubscriptionSql(').length - 1).toBe(1);
    expect(text).toContain("from './billing-read-tail-r32-sql'");
    expect(await src('executive-metric-watermarks.ts')).toContain('nativeBillingWatermarks(context)');
    expect(await src('executive-main-evidence-sources.ts')).toContain('commercialSubscriptionSelect()');
    expect(text).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  it('lowers the billing owner SELECT KPI strictly below the recorded BEFORE=60 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(59);
    expect(count).toBe(5); // r36 routed the six residual heads; r40 folded the r22 head; r41 folded the r18 lateral;
    // r42 folded the r33 head; r43 folded the r32 beta_billing head.
  });
});
