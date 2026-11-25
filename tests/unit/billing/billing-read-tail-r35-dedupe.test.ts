import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';
import { BILLING_INVOICE_BY_ID_SQL } from '../../../packages/server-billing/src/adapters/billing-read-tail-r15-sql';
import { billingProviderCostsSql, billingUsageSnapshotSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r19-sql';
import { billingUsageMeterSql, billingSeatUsageSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r20-sql';
import { billingPendingInboxSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r23-sql';
import { billingStoredLedgerSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r28-sql';
import { billingTargetMonthSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r31-sql';
import { BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL, BILLING_MONTH_INVOICE_SQL, BILLING_ROLLOUT_ELIGIBILITY_SQL } from '../../../packages/server-billing/src/adapters/billing-read-tail-r16-sql';
import { billingMetricCountSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r27-sql';
import { billingAccountProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r30-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r35 point reads captured at BEFORE=36. The SHA-256 values are
 * byte-identity oracles only, never live query sources. Every full string is
 * recomposed by the r29 head with unchanged columns/table/suffix and binds. */
const POINTS = [
  {
    name: 'invoice/by-id', full: 'SELECT * FROM billing_invoices WHERE id=$1',
    columns: '*', table: 'billing_invoices', suffix: 'id=$1', binds: '$1',
    sha256: 'a96757ef1bdfda49b48c8b7c23ec3e3504dabc8df7741d7eef417608a6a6c981',
    actual: () => BILLING_INVOICE_BY_ID_SQL,
  },
  {
    name: 'usage/snapshot', full: 'SELECT C FROM (SRC) billing_usage_snapshots WHERE holding_id=$1 AND month=$2',
    columns: 'C', table: '(SRC) billing_usage_snapshots', suffix: 'holding_id=$1 AND month=$2', binds: '$1,$2',
    sha256: 'b81442df16226630442e4879bf3ed859dbbe6630955b0ce9265878347ee5a654',
    actual: () => billingUsageSnapshotSql('C', 'SRC'),
  },
  {
    name: 'usage/meter', full: 'SELECT C\n     FROM (SRC) billing_usage_meters WHERE holding_id=$1 AND month=$2 ORDER BY source_key',
    columns: 'C\n    ', table: '(SRC) billing_usage_meters', suffix: 'holding_id=$1 AND month=$2 ORDER BY source_key', binds: '$1,$2',
    sha256: 'a7088398e809ac694f2c91678762e451c45ab39dc6ef62104bb3d2ea24d37bcf',
    actual: () => billingUsageMeterSql('C', 'SRC'),
  },
  {
    name: 'seat/rating', full: 'SELECT C FROM (SRC) billing_seat_usage WHERE holding_id=$1 AND month=$2 ORDER BY user_key',
    columns: 'C', table: '(SRC) billing_seat_usage', suffix: 'holding_id=$1 AND month=$2 ORDER BY user_key', binds: '$1,$2',
    sha256: '589ffc483989da70d1fe7bd8206be9d05b3ba818846b3a8d25dded2ba378fc06',
    actual: () => billingSeatUsageSql('C', 'SRC', ' WHERE holding_id=$1 AND month=$2 ORDER BY user_key'),
  },
  {
    name: 'seat/measured', full: 'SELECT C FROM (SRC) billing_seat_usage\n     WHERE holding_id=$1 AND month=$2',
    columns: 'C', table: '(SRC) billing_seat_usage\n    ', suffix: 'holding_id=$1 AND month=$2', binds: '$1,$2',
    sha256: 'b3a754f2ea1882a29984a0fb988a79498ce7e7600b3b7bdd9f45d404fa9a4e20',
    actual: () => billingSeatUsageSql('C', 'SRC', '\n     WHERE holding_id=$1 AND month=$2'),
  },
  {
    name: 'inbox/pending', full: "SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND TAIL",
    columns: '*', table: 'stripe_inbox', suffix: "state='pending' AND next_at<=$1 AND TAIL", binds: '$1',
    sha256: '6abe4ed05e0e98444946c1453bc39e719aa92529372d1b48c194ed4ce66b2ef2',
    actual: () => billingPendingInboxSql('TAIL'),
  },
  {
    name: 'ledger/stored', full: 'SELECT C FROM billing_evidence_storage_ledger WHERE WHERE',
    columns: 'C', table: 'billing_evidence_storage_ledger', suffix: 'WHERE', binds: '',
    sha256: '165823be482483d924b39caddedc1be987eddc6ba428054ef2bf9b28ba0c3309',
    actual: () => billingStoredLedgerSql('C', 'WHERE'),
  },
  {
    name: 'target/month', full: 'SELECT C FROM T WHERE holding_id=$1 AND month=$2 TAIL',
    columns: 'C', table: 'T', suffix: 'holding_id=$1 AND month=$2 TAIL', binds: '$1,$2',
    sha256: 'c6969dde27deda88d53af2c0d6d9679c990230c3e6b60659e7ea2a624f80b619',
    actual: () => billingTargetMonthSql('C', 'T', ' TAIL'),
  },
  {
    name: 'effective/guard', full: 'SELECT 1 FROM billing_target_rollout_approvals WHERE holding_id=$1 AND effective_month<=$2',
    columns: '1', table: 'billing_target_rollout_approvals', suffix: 'holding_id=$1 AND effective_month<=$2', binds: '$1,$2',
    sha256: '01d0259e3fbc77a8acdd2db00122116bf1549b9e15fb06d088541cfcb6472b2e',
    actual: () => billingPointReadSql('1', 'billing_target_rollout_approvals', 'holding_id=$1 AND effective_month<=$2'),
  },
  {
    name: 'provider/total', full: 'SELECT COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total\n           FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND provider=$3\n             AND statement_id=$4 AND original_currency=$5',
    columns: 'COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total\n          ', table: 'billing_target_provider_costs', suffix: 'holding_id=$1 AND month=$2 AND provider=$3\n             AND statement_id=$4 AND original_currency=$5', binds: '$1,$2,$3,$4,$5',
    sha256: '71ab86ad4eac4c761acc4c670e76634fd5c32d6cea168c72307e5c63faacb80a',
    actual: () => billingPointReadSql('COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total\n          ', 'billing_target_provider_costs', 'holding_id=$1 AND month=$2 AND provider=$3\n             AND statement_id=$4 AND original_currency=$5'),
  },
  {
    name: 'legacy/overlap', full: "SELECT 1 FROM billing_enrollments WHERE holding_id=$1 AND plan_id<>'platform_usage'\n     AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2",
    columns: '1', table: 'billing_enrollments', suffix: "holding_id=$1 AND plan_id<>'platform_usage'\n     AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2", binds: '$1,$3,$2',
    sha256: '819d19f2e771fbf17f30afeaed85722d1f8795d16b416c064e6a9b7088161b23',
    actual: () => BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL,
  },
  {
    name: 'month/invoice', full: "SELECT 1 FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id\n     WHERE i.holding_id=$1 AND p.month=$2 AND i.status<>'void'",
    columns: '1', table: 'billing_invoices i JOIN billing_periods p ON p.id=i.period_id\n    ', suffix: "i.holding_id=$1 AND p.month=$2 AND i.status<>'void'", binds: '$1,$2',
    sha256: '078f325a94db9947a5d996f6632f7269818de114a7c462da6213023c9ab79f57',
    actual: () => BILLING_MONTH_INVOICE_SQL,
  },
  {
    name: 'provider/base', full: 'SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2',
    columns: '*', table: 'billing_target_provider_costs', suffix: 'holding_id=$1 AND month=$2', binds: '$1,$2',
    sha256: '75e1eb91e1bedfc72455a8ba6a403268ae758990a02ab8929ae742a85061e4b6',
    actual: () => billingProviderCostsSql(),
  },
  {
    name: 'provider/source-key', full: 'SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND source_key=$3',
    columns: '*', table: 'billing_target_provider_costs', suffix: 'holding_id=$1 AND month=$2 AND source_key=$3', binds: '$1,$2,$3',
    sha256: '39d6af3528ec46d13db1ecee2183d66c1c8488d5b481ee862df2a4693b22fe72',
    actual: () => billingProviderCostsSql(' AND source_key=$3'),
  },
] as const;

/** The two former projection heads now share the r35 generic projection head. */
const PROJECTIONS = [
  {
    name: 'metric/count', full: 'SELECT status label,count(*)::text value FROM ',
    columns: 'status label,count(*)::text value', tail: 'FROM ', binds: '',
    sha256: '0faba3b41a13db3b265e4bd1d998bd6fc34c8222332111a11c58761dcc7669d6',
    actual: () => billingMetricCountSql('status'),
  },
  {
    name: 'account/projection', full: 'SELECT holding_id FROM billing_accounts',
    columns: 'holding_id', tail: 'FROM billing_accounts', binds: '',
    sha256: '24239c58bc1f04279cc3ab7659f2cf4912bf3ae71da566e66b46e63620057940',
    actual: () => billingAccountProjectionSql('holding_id'),
  },
] as const;

/** Every file whose own SELECT head moved, and the head file it now imports. */
const ROUTES = [
  { file: 'billing-read-tail-r15-sql.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing-read-tail-r19-sql.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing-read-tail-r20-sql.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing-read-tail-r23-sql.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing-read-tail-r28-sql.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing-read-tail-r31-sql.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing-read-tail-r16-sql.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing.ts', from: './billing-read-tail-r29-sql' },
  { file: 'billing-read-tail-r27-sql.ts', from: './billing-read-tail-r35-sql' },
  { file: 'billing-read-tail-r30-sql.ts', from: './billing-read-tail-r35-sql' },
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r35 dedupe of residual point and projection heads', () => {
  it('authors the r29 point-read and r35 projection heads exactly once as read-only text', async () => {
    const point = await src('billing-read-tail-r29-sql.ts');
    expect(point.split('SELECT').length - 1).toBe(0); // r39 routes the point read through r35
    expect(point.split('billingPointReadSql').length - 1).toBe(1);
    const projection = await src('billing-read-tail-r35-sql.ts');
    expect(projection.split('SELECT').length - 1).toBe(1);
    expect(projection.split('billingProjectionSql').length - 1).toBe(1);
    for (const helper of [point, projection]) {
      expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('recomposes every residual point read byte-for-byte with unchanged binds', () => {
    for (const oracle of POINTS) {
      expect(sha256(oracle.full), oracle.name).toBe(oracle.sha256);
      expect(billingPointReadSql(oracle.columns, oracle.table, oracle.suffix), oracle.name).toBe(oracle.full);
      expect(oracle.actual(), oracle.name).toBe(oracle.full);
      expect((oracle.full.match(/\$\d/g) ?? []).join(','), oracle.name).toBe(oracle.binds);
    }
  });

  it('recomposes both former projection heads through the single r35 head', () => {
    for (const oracle of PROJECTIONS) {
      expect(sha256(oracle.full), oracle.name).toBe(oracle.sha256);
      expect(billingProjectionSql(oracle.columns, oracle.tail), oracle.name).toBe(oracle.full);
      expect(oracle.actual(), oracle.name).toBe(oracle.full);
      expect((oracle.full.match(/\$\d/g) ?? []).join(','), oracle.name).toBe(oracle.binds);
    }
  });

  it('routes every rewritten head through the shared head and drops its inline SELECT', async () => {
    for (const { file, from } of ROUTES) {
      const text = await src(file);
      expect(text, file).toContain(`from '${from}'`);
    }
    // r36 routes the r16 UNION ALL rollout guard's two SELECT 1 heads through r29.
    for (const file of ROUTES
      .filter(({ file }) => file !== 'billing-read-tail-r16-sql.ts' && file !== 'billing.ts')
      .map(({ file }) => file)) {
      expect((await src(file)).split('SELECT ').length - 1, file).toBe(0);
    }
    // billing.ts keeps only its three protected statements (two FOR UPDATE, one
    // pg_advisory); the r37 meter/snapshot and advisory heads are owner-authored.
    expect((await src('billing.ts')).split('SELECT ').length - 1).toBe(3);
    expect((await src('billing-read-tail-r16-sql.ts')).split('SELECT').length - 1).toBe(0);
    expect(await src('billing-read-tail-r16-sql.ts')).toContain('BILLING_ROLLOUT_ELIGIBILITY_SQL');
    expect(BILLING_ROLLOUT_ELIGIBILITY_SQL.split('SELECT').length - 1).toBe(2);
    expect(await src('billing-read-tail-r15-sql.ts')).toContain('BILLING_INVOICE_BY_ID_SQL');
    expect(await src('billing-read-tail-r27-sql.ts')).toContain('billingMetricCountSql');
    expect(await src('billing-read-tail-r30-sql.ts')).toContain('billingAccountProjectionSql');
  });

  it('keeps the prior per-file head call counts with the two new billing.ts heads', async () => {
    expect((await src('billing.ts')).split('billingPointReadSql(').length - 1).toBe(7); // native retry removed selectedJob; native statement removed the provider total
    expect((await src('billing-jobs.ts')).split('billingPointReadSql(').length - 1).toBe(8);
    expect((await src('billing-target.ts')).split('billingPointReadSql(').length - 1).toBe(4);
  });

  it('leaves FOR UPDATE, advisory, native and r37 owner heads intact', async () => {
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
    expect(billing).not.toContain('billing-read-tail-r25-sql');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=36 to 24', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(5);
    expect(bare).toBe(5);
    expect(spaced).toBeLessThan(36);
    expect(bare).toBeLessThan(36);
  });
});
