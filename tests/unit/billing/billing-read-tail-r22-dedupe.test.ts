import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingWatermarkRevisionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r22-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-billing/src/adapters/${name}`, import.meta.url), 'utf8');

/** Byte-identical originals of the four executive snapshot watermark revisions. */
const ENROLLMENT = `(SELECT md5(COALESCE(string_agg(jsonb_build_array(e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at)::text,',' ORDER BY e.id),'')) FROM billing_enrollments e WHERE e.holding_id=a.holding_id)`;
const INVOICE = `(SELECT md5(COALESCE(string_agg(jsonb_build_array(i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind)::text,',' ORDER BY i.id),'')) FROM billing_invoices i WHERE i.holding_id=a.holding_id)`;
const LEGACY = `(SELECT md5(COALESCE(string_agg(jsonb_build_array(b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at)::text,',' ORDER BY b.holding_id),'')) FROM beta_billing b WHERE b.holding_id=a.holding_id)`;
const PLAN = `(SELECT md5(COALESCE(string_agg(jsonb_build_array(p.id,p.revision,p.name,p.effective_month)::text,',' ORDER BY p.id,p.revision),'')) FROM billing_plans p WHERE p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id))`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r22 parameterized family dedupe', () => {
  it('renders all four watermark revisions byte-identically to the original inline SQL', () => {
    expect(billingWatermarkRevisionSql('e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at', 'e.id', 'billing_enrollments e', 'e.holding_id=a.holding_id')).toBe(ENROLLMENT);
    expect(billingWatermarkRevisionSql('i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind', 'i.id', 'billing_invoices i', 'i.holding_id=a.holding_id')).toBe(INVOICE);
    expect(billingWatermarkRevisionSql('b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at', 'b.holding_id', 'beta_billing b', 'b.holding_id=a.holding_id')).toBe(LEGACY);
    expect(billingWatermarkRevisionSql('p.id,p.revision,p.name,p.effective_month', 'p.id,p.revision', 'billing_plans p', 'p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id)')).toBe(PLAN);
  });

  it('routes every revision call site through the shared helper with no residual inline SQL', async () => {
    const watermarks = await src('executive-metric-watermarks.ts');
    expect(watermarks.split("from './billing-read-tail-r22-sql'").length - 1).toBe(1);
    expect(watermarks.split('revision(').length - 1).toBe(4);
    for (const inline of [
      '(SELECT md5(COALESCE(string_agg(jsonb_build_array(e.id,',
      '(SELECT md5(COALESCE(string_agg(jsonb_build_array(i.id,',
      '(SELECT md5(COALESCE(string_agg(jsonb_build_array(b.subscription_id,',
      '(SELECT md5(COALESCE(string_agg(jsonb_build_array(p.id,',
    ]) expect(watermarks).not.toContain(inline);
    expect(watermarks).toContain("from './billing-read-tail-r22-sql'");
  });

  it('keeps the helper read-only and leaves advisory/FOR UPDATE reads untouched', async () => {
    for (const fragment of [ENROLLMENT, INVOICE, LEGACY, PLAN]) {
      expect(fragment).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const billing = await src('billing.ts');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain('"SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked"');
  });

  it('lowers the billing owner SELECT KPI strictly below the recorded BEFORE=69 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(69);
    expect(count).toBeLessThanOrEqual(66);
  });
});
