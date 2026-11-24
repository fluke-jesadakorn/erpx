import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingProviderCostsSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r19-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r34 point reads captured at BEFORE=42. The SHA-256 values are
 * byte-identity oracles only, never live query sources. Every full string is
 * recomposed by the r29 head with unchanged columns/table/suffix and binds. */
const ORIGINALS = [
  {
    name: 'plan/effective', full: 'SELECT * FROM billing_plans WHERE id=$1 AND effective_month<=$2 ORDER BY revision DESC LIMIT 1',
    columns: '*', table: 'billing_plans', suffix: 'id=$1 AND effective_month<=$2 ORDER BY revision DESC LIMIT 1', binds: 2,
    sha256: '133172032d4c92df9c17e98c62ec1aceeacbba99a1272eb7fa21a123e767d1cb',
  },
  {
    name: 'plan/latest', full: 'SELECT * FROM billing_plans WHERE id=$1 ORDER BY revision DESC LIMIT 1',
    columns: '*', table: 'billing_plans', suffix: 'id=$1 ORDER BY revision DESC LIMIT 1', binds: 1,
    sha256: '4e05db87764fd4d81468653e03ec21a2afc797ff713656cf9182be1eac28bd73',
  },
  {
    name: 'period/latest', full: 'SELECT * FROM billing_periods WHERE enrollment_id=$1 ORDER BY month DESC LIMIT 1',
    columns: '*', table: 'billing_periods', suffix: 'enrollment_id=$1 ORDER BY month DESC LIMIT 1', binds: 1,
    sha256: '0ff33aea9a5c2cde31bb3785e729d62909b09e7304a57a3bd515d8da7d5e1aa9',
  },
  {
    name: 'period/target', full: 'SELECT * FROM billing_periods WHERE enrollment_id=$1 AND month=$2',
    columns: '*', table: 'billing_periods', suffix: 'enrollment_id=$1 AND month=$2', binds: 2,
    sha256: 'd83db640949fcd969d4731bb4b4dd499f24ae4d1a13851e82f5da78acf707f28',
  },
  {
    name: 'invoice/event', full: 'SELECT id FROM billing_invoices WHERE holding_id=$1 AND stripe_id=$2',
    columns: 'id', table: 'billing_invoices', suffix: 'holding_id=$1 AND stripe_id=$2', binds: 2,
    sha256: 'fbfc37c92b25ee898d287218b5391d1241c93b3e7433c5613f95aa92c5d4f387',
  },
  {
    name: 'invoice/stale', full: "SELECT id FROM billing_invoices WHERE holding_id=$1 AND status IN ('open','draft') AND stripe_id IS NOT NULL AND updated_at<$2 LIMIT 10",
    columns: 'id', table: 'billing_invoices', suffix: "holding_id=$1 AND status IN ('open','draft') AND stripe_id IS NOT NULL AND updated_at<$2 LIMIT 10", binds: 2,
    sha256: 'b673eb312153b0336bddcff222bde3d21704d6e4aab49eca0d19871aecb08445',
  },
  {
    name: 'provider/base', full: 'SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2',
    columns: '*', table: 'billing_target_provider_costs', suffix: 'holding_id=$1 AND month=$2', binds: 2,
    sha256: '75e1eb91e1bedfc72455a8ba6a403268ae758990a02ab8929ae742a85061e4b6',
  },
  {
    name: 'provider/source-key', full: 'SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND source_key=$3',
    columns: '*', table: 'billing_target_provider_costs', suffix: 'holding_id=$1 AND month=$2 AND source_key=$3', binds: 3,
    sha256: '39d6af3528ec46d13db1ecee2183d66c1c8488d5b481ee862df2a4693b22fe72',
  },
  {
    name: 'operating/costs', full: 'SELECT o.category,o.amount_satang,e.source_state\n      FROM billing_target_operating_costs o\n      JOIN billing_evidence e ON e.id=o.evidence_id\n      WHERE o.holding_id=$1 AND o.month=$2',
    columns: 'o.category,o.amount_satang,e.source_state\n     ', table: 'billing_target_operating_costs o\n      JOIN billing_evidence e ON e.id=o.evidence_id\n     ', suffix: 'o.holding_id=$1 AND o.month=$2', binds: 2,
    sha256: 'c479e212a1b4678a724efdb30b4a021cc283cc0238b88170fc19a58a18c52314',
  },
  {
    name: 'final-reconciliation', full: "SELECT ready,created_at FROM billing_evidence_reconciliation_runs WHERE mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1",
    columns: 'ready,created_at', table: 'billing_evidence_reconciliation_runs', suffix: "mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1", binds: 0,
    sha256: 'ef2cf9a64b30c6132eb1bae61398686362e740c8958662fe471e8934a6628982',
  },
] as const;

/** Exact source text of each rewritten head; every one appears once in its file. */
const ROUTES = [
  { file: 'billing.ts', route: "billingPointReadSql('*', 'billing_plans', `id=$1${extra} ORDER BY revision DESC LIMIT 1`)" },
  { file: 'billing.ts', route: "billingPointReadSql('*', 'billing_periods', `enrollment_id=$1${extra}`)" },
  { file: 'billing-jobs.ts', route: "billingPointReadSql('id', 'billing_invoices', `holding_id=$1${extra}`)" },
  { file: 'billing-read-tail-r19-sql.ts', route: "billingPointReadSql('*', 'billing_target_provider_costs', `holding_id=$1 AND month=$2${extra}`)" },
  { file: 'billing-target.ts', route: "billingPointReadSql('o.category,o.amount_satang,e.source_state\\n     ','billing_target_operating_costs o\\n      JOIN billing_evidence e ON e.id=o.evidence_id\\n     ','o.holding_id=$1 AND o.month=$2')" },
  { file: 'billing-attachments.ts', route: "billingPointReadSql('ready,created_at','billing_evidence_reconciliation_runs',\"mode='final' AND upload_frozen=true AND checksums_verified=true ORDER BY created_at DESC,id DESC LIMIT 1\")" },
] as const;

/** Inline SELECT literals that the rewrite must have removed from each file. */
const REMOVED = [
  { file: 'billing.ts', literal: 'SELECT * FROM billing_plans WHERE id=$1' },
  { file: 'billing.ts', literal: 'SELECT * FROM billing_periods WHERE enrollment_id=$1' },
  { file: 'billing-jobs.ts', literal: 'SELECT id FROM billing_invoices WHERE holding_id=$1' },
  { file: 'billing-read-tail-r19-sql.ts', literal: 'SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2' },
  { file: 'billing-target.ts', literal: 'SELECT o.category,o.amount_satang,e.source_state\n      FROM billing_target_operating_costs o' },
  { file: 'billing-attachments.ts', literal: 'SELECT ready,created_at FROM billing_evidence_reconciliation_runs' },
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r34 dedupe of residual point reads', () => {
  it('authors the r29 point-read head exactly once as read-only text', async () => {
    const helper = await src('billing-read-tail-r29-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(1);
    expect(helper.split('billingPointReadSql').length - 1).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes every pre-r34 read byte-for-byte with unchanged binds', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full), oracle.name).toBe(oracle.sha256);
      expect(billingPointReadSql(oracle.columns, oracle.table, oracle.suffix), oracle.name).toBe(oracle.full);
      expect((oracle.full.match(/\$/g) ?? []).length, oracle.name).toBe(oracle.binds);
    }
    // The exported provider-cost family still renders both variants byte-identically.
    expect(billingProviderCostsSql()).toBe(ORIGINALS[6].full);
    expect(billingProviderCostsSql(' AND source_key=$3')).toBe(ORIGINALS[7].full);
  });

  it('routes every rewritten head through r29 and drops the inline literals', async () => {
    for (const { file, route } of ROUTES) expect((await src(file)).split(route).length - 1, file).toBe(1);
    for (const { file, literal } of REMOVED) expect(await src(file), file).not.toContain(literal);
    expect(await src('billing-read-tail-r19-sql.ts')).toContain("import {billingPointReadSql} from './billing-read-tail-r29-sql';");
    expect(await src('billing-attachments.ts')).toContain("import {billingPointReadSql} from './billing-read-tail-r29-sql';");
  });

  it('leaves FOR UPDATE, advisory and native reads untouched', async () => {
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain('BILLING_ADVISORY_XACT_LOCK_SQL');
    expect(billing).toContain('WHERE holding_id=$1 AND month=$2 FOR UPDATE');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain('BILLING_ADVISORY_UNLOCK_SQL');
    expect(jobs).toContain('pg_try_advisory_lock');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=42 to 24', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(16);
    expect(bare).toBe(16);
    expect(spaced).toBeLessThan(42);
    expect(bare).toBeLessThan(42);
  });
});
