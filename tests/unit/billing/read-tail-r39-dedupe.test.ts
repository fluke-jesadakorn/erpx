import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (path: string) => readFile(new URL(`${path}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r39 point reads captured at BEFORE=10 (HEAD 77bc573d3). The
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const ORACLES = [
  { name: 'receipt/by-id', columns: '*', table: 'billing_receipts', suffix: 'id=$1',
    full: 'SELECT * FROM billing_receipts WHERE id=$1',
    sha256: '8c8cd9bb34ae58040d7b0e690a6d9d3cdd3b1ba79cdeb9e8342cec99f1df0fbe' },
  { name: 'invoice/id-fields', columns: 'id,total_satang,status,stripe_id', table: 'billing_invoices', suffix: 'id=$1',
    full: 'SELECT id,total_satang,status,stripe_id FROM billing_invoices WHERE id=$1',
    sha256: 'bbdee083a1b236222b804747181ee9830a76583d07cb8d415b6a22ac2b87bbcb' },
  { name: 'job/id-and-holding', columns: '*', table: 'billing_jobs', suffix: 'id=$1 AND holding_id=$2',
    full: 'SELECT * FROM billing_jobs WHERE id=$1 AND holding_id=$2',
    sha256: '6f37e40564012526a62bae0c0c9e365f16d1f7cca68e8fb9e4e8996336c9fa8d' },
  { name: 'enrollment/active', columns: '*', table: 'billing_enrollments', suffix: 'holding_id=$1 AND ended_at IS NULL',
    full: 'SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL',
    sha256: '247bb55406ea188b3a60cbe1c04afcf7cd7405ff8019305dcfc102bda9c2b657' },
  { name: 'invoice/auto-open', columns: '*', table: 'billing_invoices', suffix: "holding_id=$1 AND status='open' AND payment_mode='auto'",
    full: "SELECT * FROM billing_invoices WHERE holding_id=$1 AND status='open' AND payment_mode='auto'",
    sha256: '7c981bb94cfafb22a3215faba3c513f05a3dcdb7496cbd0e4262fd60da63bcfd' },
  { name: 'job/pending-queue', columns: '*', table: 'billing_jobs', suffix: "holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10",
    full: "SELECT * FROM billing_jobs WHERE holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10",
    sha256: '9be0989e939047e1a584595e1e842da9e3941fb92324f9f8e41326e1c7e97b1b' },
  { name: 'assignment/effective', columns: '*', table: 'billing_target_assignments', suffix: 'holding_id=$1\n     AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1',
    full: 'SELECT * FROM billing_target_assignments WHERE holding_id=$1\n     AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1',
    sha256: 'bafb80b3701e361833666967fe4d792b90e2803c6268a786c2c9abeb40fcce26' },
  { name: 'account/customer-id', columns: 'customer_id', table: 'billing_accounts', suffix: 'holding_id=$2',
    full: 'SELECT customer_id FROM billing_accounts WHERE holding_id=$2',
    sha256: '8b2f472fb5dd92f9c37153a1d50b06e0d213c2f578dc2f772c556d14997cbfc7' },
  { name: 'rollout/effective-guard', columns: '1', table: 'billing_target_rollout_approvals', suffix: 'holding_id=$1 AND effective_month<=$2',
    full: 'SELECT 1 FROM billing_target_rollout_approvals WHERE holding_id=$1 AND effective_month<=$2',
    sha256: '01d0259e3fbc77a8acdd2db00122116bf1549b9e15fb06d088541cfcb6472b2e' },
  { name: 'provider/total-with-newlines', columns: 'COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total\n          ',
    table: 'billing_target_provider_costs', suffix: 'holding_id=$1 AND month=$2 AND provider=$3\n             AND statement_id=$4 AND original_currency=$5',
    full: 'SELECT COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total\n           FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND provider=$3\n             AND statement_id=$4 AND original_currency=$5',
    sha256: '71ab86ad4eac4c761acc4c670e76634fd5c32d6cea168c72307e5c63faacb80a' },
  { name: 'enrollment/correlated-guard', columns: '1', table: 'billing_enrollments e', suffix: 'e.holding_id=a.holding_id',
    full: 'SELECT 1 FROM billing_enrollments e WHERE e.holding_id=a.holding_id',
    sha256: 'd7d02a1971813b23d9e9e7e9879937c1c4ce409af7e8e75f3ba7738c78258ad9' },
  { name: 'enrollment/plan-subquery', columns: 'e.plan_id', table: 'billing_enrollments e', suffix: 'e.holding_id=a.holding_id',
    full: 'SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id',
    sha256: '18752967d2cd1b2f4345a3caee51ae500790dde965c907e858202ebda2e5535f' },
  { name: 'account/by-holding', columns: '*', table: 'billing_accounts', suffix: 'holding_id=$1',
    full: 'SELECT * FROM billing_accounts WHERE holding_id=$1',
    sha256: '43d922b694527f900c0f047f99caed09e2a5e653efa16f5034ab72a28812c685' },
  { name: 'account/payment-fields', columns: 'payment_mode,consent_at,card_json', table: 'billing_accounts', suffix: 'holding_id=$1',
    full: 'SELECT payment_mode,consent_at,card_json FROM billing_accounts WHERE holding_id=$1',
    sha256: '7ddea2b2b3ff981e588e34bfe063258f65f69444d10eee42001d546b508ee6f8' },
  { name: 'sandbox/holding-guard', columns: '1', table: 'billing_target_sandbox_holdings', suffix: 'holding_id=$1',
    full: 'SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1',
    sha256: '69dd32a65214f1a4625ce47358d6f5a4764a1d6d6e81b9f2c259d4624230367e' },
  { name: 'reconciliation/final-ready', columns: 'ready,created_at', table: 'billing_evidence_reconciliation_runs',
    suffix: "mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1",
    full: "SELECT ready,created_at FROM billing_evidence_reconciliation_runs WHERE mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1",
    sha256: 'ef2cf9a64b30c6132eb1bae61398686362e740c8958662fe471e8934a6628982' },
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r39 dedupe of the r29 point-read head', () => {
  it('turns the r29 head into a strict specialization of the r35 projection head', async () => {
    const point = await src('adapters/billing-read-tail-r29-sql.ts');
    const projection = await src('adapters/billing-read-tail-r35-sql.ts');
    expect(point.split('SELECT').length - 1).toBe(0);
    expect(point.split('billingPointReadSql').length - 1).toBe(1);
    expect(point).toContain("import { billingProjectionSql } from './billing-read-tail-r35-sql';");
    expect(point).toContain('billingProjectionSql(columns, `FROM ${table} WHERE ${suffix}`)');
    expect(point).toContain('export const billingPointReadSql = (columns: string, table: string, suffix: string) =>');
    expect(projection.split('SELECT').length - 1).toBe(1);
    expect(projection.split('billingProjectionSql').length - 1).toBe(1);
    for (const helper of [point, projection]) {
      expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('recomposes every representative r29 caller byte-for-byte with unchanged binds', () => {
    for (const oracle of ORACLES) {
      const emitted = billingPointReadSql(oracle.columns, oracle.table, oracle.suffix);
      expect(sha256(oracle.full), oracle.name).toBe(oracle.sha256);
      expect(emitted, oracle.name).toBe(oracle.full);
      expect(billingProjectionSql(oracle.columns, `FROM ${oracle.table} WHERE ${oracle.suffix}`), oracle.name).toBe(oracle.full);
      expect((oracle.full.match(/\$\d/g) ?? []).join(','), oracle.name).toBe((oracle.suffix.match(/\$\d/g) ?? []).join(','));
    }
  });

  it('leaves every call site routed through the unchanged helper', async () => {
    let calls = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      calls += text.split('billingPointReadSql(').length - 1;
    }
    expect(calls).toBe(35);
    const billing = await src('adapters/billing.ts');
    const target = await src('adapters/billing-target.ts');
    expect(billing).toContain("import {billingPointReadSql} from './billing-read-tail-r29-sql';");
    expect(target).toContain("import {billingPointReadSql} from './billing-read-tail-r29-sql';");
  });

  it('leaves FOR UPDATE, advisory and native owner locks untouched', async () => {
    const billing = await src('adapters/billing.ts');
    const jobs = await src('adapters/billing-jobs.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(billing).toContain("c.query<Record<string,unknown>>(\"SELECT pg_advisory_xact_lock(hashtextextended('billing-plans',0))\",[])");
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=10 to 9', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(9);
    expect(bare).toBe(9);
    expect(spaced).toBeLessThan(10);
    expect(bare).toBeLessThan(10);
  });
});
