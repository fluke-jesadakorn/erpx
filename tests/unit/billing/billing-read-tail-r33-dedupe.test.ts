import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingEnrollmentSubscriptionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r33-sql';
import { billingPendingInboxSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r23-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r33 residual point reads captured at BEFORE=48 (HEAD f594d226).
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const POINTS = [
  {
    file: 'billing.ts',
    full: 'SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL',
    columns: '*', table: 'billing_enrollments', suffix: 'holding_id=$1 AND ended_at IS NULL', binds: 1,
    sha256: '247bb55406ea188b3a60cbe1c04afcf7cd7405ff8019305dcfc102bda9c2b657',
    routed: "billingPointReadSql('*','billing_enrollments','holding_id=$1 AND ended_at IS NULL')",
    // The FOR UPDATE variant at billing.ts:34 deliberately retains this text as a prefix.
    inlineAfter: 1,
  },
  {
    file: 'billing.ts',
    full: "SELECT * FROM billing_invoices WHERE holding_id=$1 AND status='open' AND payment_mode='auto'",
    columns: '*', table: 'billing_invoices', suffix: "holding_id=$1 AND status='open' AND payment_mode='auto'", binds: 1,
    sha256: '7c981bb94cfafb22a3215faba3c513f05a3dcdb7496cbd0e4262fd60da63bcfd',
    routed: "billingPointReadSql('*','billing_invoices',\"holding_id=$1 AND status='open' AND payment_mode='auto'\")",
    inlineAfter: 0,
  },
  {
    file: 'billing-jobs.ts',
    full: "SELECT * FROM billing_jobs WHERE holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10",
    columns: '*', table: 'billing_jobs', suffix: "holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10", binds: 2,
    sha256: '9be0989e939047e1a584595e1e842da9e3941fb92324f9f8e41326e1c7e97b1b',
    routed: "billingPointReadSql('*','billing_jobs',\"holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10\")",
    inlineAfter: 0,
  },
  {
    file: 'billing-target.ts',
    full: 'SELECT * FROM billing_target_assignments WHERE holding_id=$1\n     AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1',
    columns: '*', table: 'billing_target_assignments', suffix: 'holding_id=$1\n     AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1', binds: 2,
    sha256: 'bafb80b3701e361833666967fe4d792b90e2803c6268a786c2c9abeb40fcce26',
    routed: "billingPointReadSql('*','billing_target_assignments',`holding_id=$1\n     AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1`)",
    inlineAfter: 0,
  },
  {
    file: 'billing-jobs.ts',
    full: 'SELECT customer_id FROM billing_accounts WHERE holding_id=$2',
    columns: 'customer_id', table: 'billing_accounts', suffix: 'holding_id=$2', binds: 1,
    sha256: '8b2f472fb5dd92f9c37153a1d50b06e0d213c2f578dc2f772c556d14997cbfc7',
    routed: "billingPointReadSql('customer_id','billing_accounts','holding_id=$2')",
    inlineAfter: 0,
  },
] as const;

/** Verbatim pre-r33 enrollment subscription heads (r18 lateral resolved).
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const HEADS = [
  {
    file: 'executive-main-evidence-sources.ts',
    full: `      SELECT 'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,
        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,
        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version
      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id
      LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
      WHERE a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)`,
    projection: "'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,\n        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,\n        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version",
    from: '      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id',
    holding: 'a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)',
    pad: '      ',
    sha256: '78959199cfc195d6d8163ffe8cb7cc331772d578df28287b3b3f53c0434c0461',
    binds: '?,?,?',
  },
  {
    file: 'executive-metric-projections.ts',
    full: `        SELECT e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status
        FROM billing_enrollments e
        LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL
          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)`,
    projection: "e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status",
    from: '        FROM billing_enrollments e',
    holding: "e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL\n          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)",
    pad: '        ',
    sha256: '6bcd54a6afc84e2dd94c0c07e95d13e5eceacf252f8ae48863ab5bf97bacd5ad',
    binds: '?,?,?',
  },
] as const;

/** The pending stripe_inbox drain head, including its correlated scalar tail. */
const PENDING_EVENTS = {
  full: "SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$2) ORDER BY created_at,id LIMIT 50",
  tail: "`payload_json::jsonb #>> '{data,object,customer}'=(${billingPointReadSql('customer_id','billing_accounts','holding_id=$2')}) ORDER BY created_at,id LIMIT 50`",
  sha256: '0a6c6b58dc4b06d24461b35f1a98ccf315cdf683aa179784463720f6f6b05206',
} as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r33 residual dedupe', () => {
  it('authors the enrollment subscription head exactly once as read-only text reusing r18 lateral', async () => {
    const helper = await src('billing-read-tail-r33-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(1);
    expect(helper.split('billingEnrollmentSubscriptionSql').length - 1).toBe(1);
    expect(helper).toContain("import { BILLING_PLAN_NAME_LATERAL_SQL } from './billing-read-tail-r18-sql';");
    expect(helper).not.toContain('LEFT JOIN LATERAL');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes both enrollment heads byte-for-byte with unchanged binds', () => {
    for (const oracle of HEADS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(billingEnrollmentSubscriptionSql(oracle.projection, oracle.from, oracle.holding, oracle.pad)).toBe(oracle.full);
      expect((oracle.full.match(/\?/g) ?? []).join(',')).toBe(oracle.binds);
      expect(oracle.full).toContain('LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE');
    }
  });

  it('routes both executive readers through the shared head with unchanged binds', async () => {
    for (const oracle of HEADS) {
      const text = await src(oracle.file);
      expect(text).toContain("from './billing-read-tail-r33-sql'");
      expect(text.split('billingEnrollmentSubscriptionSql(').length - 1).toBe(1);
      expect(text).not.toContain('${BILLING_PLAN_NAME_LATERAL_SQL}');
      expect(text).not.toContain("from './billing-read-tail-r18-sql'");
      expect(text).not.toContain(oracle.full.slice(0, oracle.pad.length + 8));
      expect(text).toContain(oracle.file === 'executive-main-evidence-sources.ts'
        ? '[actor.holdingId, now, now, actor.holdingId, now]'
        : 'values = [a.holdingId, now, now, a.holdingId, now];');
    }
  });

  it('recomposes all five residual point reads byte-for-byte with unchanged binds', () => {
    for (const oracle of POINTS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(billingPointReadSql(oracle.columns, oracle.table, oracle.suffix)).toBe(oracle.full);
      expect((oracle.full.match(/\$\d/g) ?? []).length).toBe(oracle.binds);
    }
  });

  it('removes the inline residual literals and routes every call site through the shared helpers', async () => {
    const billing = await src('billing.ts');
    const jobs = await src('billing-jobs.ts');
    const target = await src('billing-target.ts');
    for (const oracle of POINTS) {
      const text = oracle.file === 'billing.ts' ? billing : oracle.file === 'billing-jobs.ts' ? jobs : target;
      expect(text.split(oracle.full).length - 1).toBe(oracle.inlineAfter);
      expect(text.split(oracle.routed).length - 1).toBe(1);
    }
    expect(billing.split('billingPointReadSql(').length - 1).toBe(9); // r35 effective-month guard + provider total
    expect(jobs.split('billingPointReadSql(').length - 1).toBe(8);
    expect(target.split('billingPointReadSql(').length - 1).toBe(4);
  });

  it('preserves the pendingEvents scalar tail with $2 index and wrapping parens', () => {
    expect(sha256(PENDING_EVENTS.full)).toBe(PENDING_EVENTS.sha256);
    expect(billingPendingInboxSql(`payload_json::jsonb #>> '{data,object,customer}'=(${billingPointReadSql('customer_id','billing_accounts','holding_id=$2')}) ORDER BY created_at,id LIMIT 50`)).toBe(PENDING_EVENTS.full);
    expect(PENDING_EVENTS.tail).toContain("billingPointReadSql('customer_id','billing_accounts','holding_id=$2')");
    expect(PENDING_EVENTS.full).toContain('=(SELECT customer_id FROM billing_accounts WHERE holding_id=$2)');
    expect((PENDING_EVENTS.full.match(/\$\d/g) ?? []).join(',')).toBe('$1,$2');
  });

  it('keeps FOR UPDATE, advisory and native owner reads untouched', async () => {
    const billing = await src('billing.ts');
    const jobs = await src('billing-jobs.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(billing).toContain('nativeBillingOwnerQuery');
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
    expect(billing).not.toContain('billing-read-tail-r25-sql');
    expect(await src('billing-read-tail-r18-sql.ts')).toContain('BILLING_PLAN_NAME_LATERAL_SQL');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=48 to 42', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(13);
    expect(bare).toBe(13);
    expect(spaced).toBeLessThan(48);
    expect(bare).toBeLessThan(48);
  });
});
