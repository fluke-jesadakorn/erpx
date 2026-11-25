import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingWatermarkRevisionSql as revision } from '../../../packages/server-billing/src/adapters/billing-read-tail-r22-sql';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const qmarks = (s: string) => (s.match(/\?/g) ?? []).length;

/** Verbatim pre-r38 SELECT heads captured at BEFORE=13 (base 777ce3b0e9). The
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const HEADS = [
  {
    name: 'overdueInvoiceEvidence',
    file: 'executive-main-evidence-sources.ts',
    full: `SELECT i.id,i.kind,i.status,i.total_satang,i.due_at,i.updated_at,p.month
    FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id AND p.holding_id=i.holding_id
    WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?`,
    sha256: '824abaa5dddbe6083e75f9f9bb01cd1f944b1947c3113f3a7ac94d0290e0026a',
    binds: 4,
    actual: () => billingProjectionSql(`i.id,i.kind,i.status,i.total_satang,i.due_at,i.updated_at,p.month
   `, `FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id AND p.holding_id=i.holding_id
    WHERE i.holding_id=? AND i.status IN ('open','uncollectible') AND i.due_at>=? AND i.due_at<? AND i.due_at<?`),
    route: 'billingProjectionSql(`i.id,i.kind,i.status,i.total_satang,i.due_at,i.updated_at,p.month',
  },
  {
    name: 'billingWatermarksAccount',
    file: 'executive-metric-watermarks.ts',
    full: `SELECT a.version::text account_version,
        ${revision('e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at','e.id','billing_enrollments e','e.holding_id=a.holding_id')} enrollment_revision,
        ${revision('i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind','i.id','billing_invoices i','i.holding_id=a.holding_id')} invoice_revision,
        ${revision('b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at','b.holding_id','beta_billing b','b.holding_id=a.holding_id')} legacy_subscription_revision,
        ${revision('p.id,p.revision,p.name,p.effective_month','p.id,p.revision','billing_plans p',`p.id IN (${billingPointReadSql('e.plan_id','billing_enrollments e','e.holding_id=a.holding_id')})`)} plan_revision
        FROM billing_accounts a WHERE a.holding_id=?`,
    sha256: 'cbc8b9502898c83c92c8d362f92fa5eab78e6a15243f518a50cf61c4550d0d02',
    binds: 1,
    actual: () => billingProjectionSql(`a.version::text account_version,
        ${revision('e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at','e.id','billing_enrollments e','e.holding_id=a.holding_id')} enrollment_revision,
        ${revision('i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind','i.id','billing_invoices i','i.holding_id=a.holding_id')} invoice_revision,
        ${revision('b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at','b.holding_id','beta_billing b','b.holding_id=a.holding_id')} legacy_subscription_revision,
        ${revision('p.id,p.revision,p.name,p.effective_month','p.id,p.revision','billing_plans p',`p.id IN (${billingPointReadSql('e.plan_id','billing_enrollments e','e.holding_id=a.holding_id')})`)} plan_revision
       `, 'FROM billing_accounts a WHERE a.holding_id=?'),
    route: 'sourceDb.prepare(billingProjectionSql(`a.version::text account_version,',
  },
  {
    name: 'pendingScanCount',
    file: 'billing-attachments.ts',
    full: "SELECT count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint pending",
    sha256: '685ab5f0dcf3800521c505aa8b70e9f87b7c576c8dfddaf7b74a4282a642d88e',
    binds: 0,
    actual: () => billingProjectionSql("count(*) FILTER (WHERE source_state IN ('legacy_unscanned','quarantined','scanning','scan_error'))::bigint", 'pending'),
    route: "billingProjectionSql(\"count(*) FILTER (WHERE source_state IN ('legacy_unscanned'",
  },
] as const;

/** Divergent suffixes kept at the two pending-scan call sites and the overdue
 * point read. None of them was folded into the shared head. */
const SUFFIXES = [
  {
    file: 'billing-attachments.ts',
    text: ",count(*) FILTER (WHERE source_state='malware_rejected')::bigint sealed_malware FROM billing_evidence",
    use: '${BILLING_PENDING_SCAN_COUNT_SQL},count(*) FILTER',
  },
  {
    file: 'billing-attachments.ts',
    text: '${BILLING_PENDING_SCAN_COUNT_SQL} FROM billing_evidence',
    use: '${BILLING_PENDING_SCAN_COUNT_SQL} FROM billing_evidence',
  },
  {
    file: 'executive-main-evidence-sources.ts',
    text: ' AND i.id=? LIMIT 1',
    use: '${overdueInvoiceSource()} AND i.id=? LIMIT 1',
  },
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r38 dedupe of three residual exec heads', () => {
  it('keeps the r35 projection head authored exactly once as read-only text', async () => {
    const helper = await src('billing-read-tail-r35-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(1);
    expect(helper.split('billingProjectionSql').length - 1).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes all three pre-r38 heads byte-for-byte with unchanged binds', () => {
    for (const head of HEADS) {
      expect(sha256(head.full), head.name).toBe(head.sha256);
      expect(head.actual(), head.name).toBe(head.full);
      expect(qmarks(head.full), head.name).toBe(head.binds);
    }
    // The overdue point read appends one more bind (id) to the four head binds.
    expect(qmarks(HEADS[0].full + ' AND i.id=? LIMIT 1')).toBe(5);
  });

  it('routes each head through the shared head and drops every inline SELECT', async () => {
    for (const head of HEADS) {
      const text = await src(head.file);
      expect(text, head.name).toContain(head.route);
      expect(text.split('SELECT').length - 1, head.name).toBe(0);
      expect(text, head.name).toContain("from './billing-read-tail-r35-sql'");
    }
    // The point read keeps its five binds in the original order (id last).
    expect(await src('executive-main-evidence-sources.ts'))
      .toContain('.bind(actor.holdingId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, now, id)');
    // The watermark reader still binds only the holding id.
    expect(await src('executive-metric-watermarks.ts')).toContain(')).bind(holdingId)');
  });

  it('keeps the divergent pending-scan suffixes and the overdue id suffix', async () => {
    for (const suffix of SUFFIXES) {
      const text = await src(suffix.file);
      expect(text.split(suffix.text).length - 1, suffix.text).toBe(1);
      expect(text, suffix.text).toContain(suffix.use);
    }
  });

  it('leaves FOR UPDATE, advisory and try-lock reads untouched', async () => {
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=13 to 10', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(7);
    expect(bare).toBe(7);
    expect(spaced).toBeLessThan(13);
    expect(bare).toBeLessThan(13);
  });
});
