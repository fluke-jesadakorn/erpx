import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';
import { billingStoredLedgerSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r28-sql';
import { BILLING_ROLLOUT_ELIGIBILITY_SQL } from '../../../packages/server-billing/src/adapters/billing-read-tail-r16-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

const DUP_OBJECTS = billingStoredLedgerSql('object_key', "state='stored' GROUP BY object_key HAVING count(*)>1");
const UNIONS = '\n     UNION ALL ';

/** Verbatim pre-r36 SELECT heads captured at BEFORE=24 (base a79b5abc4). The
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const HEADS = [
  {
    name: 'paidTargetInvoices', file: 'billing-jobs.ts',
    full: `SELECT i.id FROM billing_invoices i
         LEFT JOIN billing_test_finance_projection f ON f.billing_invoice_id=i.id
         WHERE i.holding_id=$1 AND i.rating_model='target' AND i.status='paid'
           AND (f.synced_at IS NULL OR f.synced_at<$2)
         ORDER BY i.updated_at DESC LIMIT 10`,
    sha256: '5689520b9184767df8ab264a29ab19aa4eebf1b86bacc4ad7ed59326d2bb0f11',
    binds: '$1,$2',
    actual: () => billingProjectionSql('i.id', `FROM billing_invoices i
         LEFT JOIN billing_test_finance_projection f ON f.billing_invoice_id=i.id
         WHERE i.holding_id=$1 AND i.rating_model='target' AND i.status='paid'
           AND (f.synced_at IS NULL OR f.synced_at<$2)
         ORDER BY i.updated_at DESC LIMIT 10`),
    route: "billingProjectionSql('i.id',`FROM billing_invoices i",
  },
  {
    name: 'sources', file: 'billing-attachments.ts',
    full: 'SELECT id,holding_id,object_key,size,sha256,source_state FROM billing_evidence ORDER BY id',
    sha256: 'b324bbb47122a1652e13fa5c0a1cd81e572b7c736dc05b16be024cce3d50b759',
    binds: '',
    actual: () => billingProjectionSql('id,holding_id,object_key,size,sha256,source_state', 'FROM billing_evidence ORDER BY id'),
    route: "billingProjectionSql('id,holding_id,object_key,size,sha256,source_state','FROM billing_evidence ORDER BY id')",
  },
  {
    name: 'storedTotalsInner', file: 'billing-attachments.ts',
    full: `(SELECT count(*)::bigint FROM (${DUP_OBJECTS}) duplicate_objects)`,
    sha256: '118283e2c02bacada5d48713b0fcf7ed5d475ecf06ff22215c19cd757478e5cc',
    binds: '',
    actual: () => `(${billingProjectionSql('count(*)::bigint', `FROM (${DUP_OBJECTS}) duplicate_objects`)})`,
    route: "billingProjectionSql('count(*)::bigint',",
  },
  {
    name: 'subscriptionSelect', file: 'executive-main-evidence-sources.ts',
    full: 'SELECT source_kind,record_id,plan_name,status,started_at,effective_at,record_version FROM subscriptions',
    sha256: '0cde7c8a805c5e46b84bb34ee12a9668013a856f3c38b744ed5099deeb972c33',
    binds: '',
    actual: () => billingProjectionSql('source_kind,record_id,plan_name,status,started_at,effective_at,record_version', 'FROM subscriptions'),
    route: "billingProjectionSql('source_kind,record_id,plan_name,status,started_at,effective_at,record_version','FROM subscriptions')",
  },
  {
    name: 'legacyEventsTail', file: 'billing-jobs.ts',
    full: "NOT EXISTS(SELECT 1 FROM billing_accounts a WHERE a.customer_id=stripe_inbox.payload_json::jsonb #>> '{data,object,customer}') LIMIT 20",
    sha256: 'b1399f88f61743e7ab477dc0658b50f7d796ee221f7fe7dab06f003066d7ae5c',
    binds: '',
    actual: () => `NOT EXISTS(${billingPointReadSql('1', 'billing_accounts a', "a.customer_id=stripe_inbox.payload_json::jsonb #>> '{data,object,customer}')")} LIMIT 20`,
    route: "billingPointReadSql('1','billing_accounts a'",
  },
  {
    name: 'planRevisionFilter', file: 'executive-metric-watermarks.ts',
    full: 'p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id)',
    sha256: '0e15c0f1b815194bf77c6fc74fd38bf39fcf2fc5237c1bb560d6c910860ac29c',
    binds: '',
    actual: () => `p.id IN (${billingPointReadSql('e.plan_id', 'billing_enrollments e', 'e.holding_id=a.holding_id')})`,
    route: "billingPointReadSql('e.plan_id','billing_enrollments e','e.holding_id=a.holding_id')",
  },
  {
    name: 'rolloutGuard1', file: 'billing-read-tail-r16-sql.ts',
    full: 'SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1',
    sha256: '69dd32a65214f1a4625ce47358d6f5a4764a1d6d6e81b9f2c259d4624230367e',
    binds: '$1',
    actual: () => BILLING_ROLLOUT_ELIGIBILITY_SQL.slice(0, BILLING_ROLLOUT_ELIGIBILITY_SQL.indexOf(UNIONS)),
    route: "billingPointReadSql('1', 'billing_target_sandbox_holdings',",
  },
  {
    name: 'rolloutGuard2', file: 'billing-read-tail-r16-sql.ts',
    full: `SELECT 1 FROM billing_target_rollout_approvals
     WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1`,
    sha256: 'dae4ef1c3ad6d9f7d1d756697fc9a6a24440655cb392700efbda45d89e97db03',
    binds: '$1,$2',
    actual: () => BILLING_ROLLOUT_ELIGIBILITY_SQL.slice(BILLING_ROLLOUT_ELIGIBILITY_SQL.indexOf(UNIONS) + UNIONS.length),
    route: "billingPointReadSql('1', `billing_target_rollout_approvals",
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

describe('billing read-tail r36 dedupe of residual read heads', () => {
  it('authors the r29/r35 heads exactly once and keeps them read-only text', async () => {
    for (const [file, fn] of [['billing-read-tail-r29-sql.ts', 'billingPointReadSql'], ['billing-read-tail-r35-sql.ts', 'billingProjectionSql']] as const) {
      const helper = await src(file);
      expect(helper.split('SELECT').length - 1).toBe(1);
      expect(helper.split(fn).length - 1).toBe(1);
      expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('recomposes all eight pre-r36 heads byte-for-byte with unchanged binds', () => {
    for (const head of HEADS) {
      expect(sha256(head.full), head.name).toBe(head.sha256);
      expect(head.actual(), head.name).toBe(head.full);
      expect((head.full.match(/\$\d/g) ?? []).join(','), head.name).toBe(head.binds);
    }
    expect(BILLING_ROLLOUT_ELIGIBILITY_SQL).toBe(HEADS[6].full + UNIONS + HEADS[7].full);
    expect(BILLING_ROLLOUT_ELIGIBILITY_SQL.split('SELECT').length - 1).toBe(2);
  });

  it('removes every replaced inline head and routes each call site through the shared heads', async () => {
    for (const head of HEADS) {
      const text = await src(head.file);
      expect(text.split(head.full).length - 1, head.name).toBe(0);
      expect(text, head.name).toContain(head.route);
    }
    for (const file of ['billing-jobs.ts', 'billing-attachments.ts', 'executive-main-evidence-sources.ts']) {
      expect(await src(file), file).toContain("from './billing-read-tail-r35-sql'");
    }
    for (const file of ['executive-metric-watermarks.ts', 'billing-read-tail-r16-sql.ts']) {
      expect(await src(file), file).toContain("from './billing-read-tail-r29-sql'");
    }
    expect((await src('billing-jobs.ts')).split('billingPointReadSql(').length - 1).toBe(8); // + legacyEvents inner guard
    expect((await src('billing-read-tail-r16-sql.ts')).split('SELECT').length - 1).toBe(0);
  });

  it('leaves FOR UPDATE, advisory and native owner reads untouched', async () => {
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(billing).toContain('nativeBillingOwnerQuery');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
    expect(jobs.split('SELECT ').length - 1).toBe(1); // advisory only
  });

  it('lowers the billing owner SELECT KPI from BEFORE=16 to 13', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(13);
    expect(bare).toBe(13);
    expect(spaced).toBeLessThan(24);
    expect(bare).toBeLessThan(24);
  });
});
