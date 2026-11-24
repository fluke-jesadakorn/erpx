import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-billing/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');

const GUARD = 'SELECT 1 FROM ${table} WHERE holding_id=$1 AND effective_month<=$2';
const PLAN = 'SELECT * FROM billing_plans WHERE id=$1${extra} ORDER BY revision DESC LIMIT 1';
const PERIOD = 'SELECT * FROM billing_periods WHERE enrollment_id=$1${extra}';
const PENDING_SCAN = "SELECT count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint pending";
const INVOICE_ID = 'SELECT id FROM billing_invoices WHERE holding_id=$1${extra}';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r17 alias-parameterized family dedupe', () => {
  it('declares each shared SQL text exactly once', async () => {
    const billing = await src('billing.ts');
    expect(billing.split(GUARD).length - 1).toBe(1);
    expect(billing.split(PLAN).length - 1).toBe(1);
    expect(billing.split(PERIOD).length - 1).toBe(1);
    const attachments = await src('billing-attachments.ts');
    expect(attachments.split(PENDING_SCAN).length - 1).toBe(1);
    const jobs = await src('billing-jobs.ts');
    expect(jobs.split(INVOICE_ID).length - 1).toBe(1);
  });

  it('routes every family call site through the shared fragment with byte-identical SQL', async () => {
    const billing = await src('billing.ts');
    for (const call of [
      'billingEffectiveMonthGuardSql("billing_target_rollout_approvals")',
      'billingEffectiveMonthGuardSql("billing_target_assignments")',
      'billingPlanByRevisionSql(" AND effective_month<=$2")',
      'billingPlanByRevisionSql()',
      'billingPeriodByEnrollmentSql(" ORDER BY month DESC LIMIT 1")',
      'billingPeriodByEnrollmentSql(" AND month=$2")',
    ]) expect(billing.split(call).length - 1).toBe(1);
    for (const inline of [
      '"SELECT 1 FROM billing_target_rollout_approvals WHERE holding_id=$1 AND effective_month<=$2"',
      '"SELECT 1 FROM billing_target_assignments WHERE holding_id=$1 AND effective_month<=$2"',
      '"SELECT * FROM billing_plans WHERE id=$1 AND effective_month<=$2 ORDER BY revision DESC LIMIT 1"',
      '"SELECT * FROM billing_plans WHERE id=$1 ORDER BY revision DESC LIMIT 1"',
      '"SELECT * FROM billing_periods WHERE enrollment_id=$1 ORDER BY month DESC LIMIT 1"',
      '"SELECT * FROM billing_periods WHERE enrollment_id=$1 AND month=$2"',
    ]) expect(billing).not.toContain(inline);

    const attachments = await src('billing-attachments.ts');
    expect(attachments.split('${BILLING_PENDING_SCAN_COUNT_SQL},count(*) FILTER').length - 1).toBe(1);
    expect(attachments.split('${BILLING_PENDING_SCAN_COUNT_SQL} FROM billing_evidence').length - 1).toBe(1);
    expect(attachments).not.toContain('`SELECT count(*) FILTER (WHERE source_state IN (');

    const jobs = await src('billing-jobs.ts');
    expect(jobs.split('billingInvoiceIdSql(" AND stripe_id=$2")').length - 1).toBe(1);
    expect(jobs.split('billingInvoiceIdSql(" AND status IN (\'open\',\'draft\') AND stripe_id IS NOT NULL AND updated_at<$2 LIMIT 10")').length - 1).toBe(1);
    expect(jobs).not.toContain('"SELECT id FROM billing_invoices WHERE holding_id=$1 AND stripe_id=$2"');
    expect(jobs).not.toContain('"SELECT id FROM billing_invoices WHERE holding_id=$1 AND status IN (');
  });

  it('keeps the fragments read-only and leaves advisory/FOR UPDATE reads untouched', async () => {
    const billing = await src('billing.ts');
    const jobs = await src('billing-jobs.ts');
    for (const fragment of [GUARD, PLAN, PERIOD, PENDING_SCAN, INVOICE_ID]) {
      expect(fragment).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    expect(billing).toContain('"SELECT pg_advisory_xact_lock(hashtextextended($1,0))"');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(jobs).toContain('"SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked"');
  });

  it('lowers the billing owner SELECT KPI strictly below the r16 after=80 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBe(71);
    expect(count).toBeLessThan(75);
  });
});
