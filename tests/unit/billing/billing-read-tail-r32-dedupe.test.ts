import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingPointReadSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r29-sql';
import { billingLegacySubscriptionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r32-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r32 legacy beta_billing heads captured at BEFORE=53 (HEAD 6a54b8d5).
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const LEGACY = [
  {
    file: 'executive-main-evidence-sources.ts',
    full: `      SELECT 'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,
        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version
      FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
        AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)`,
    projection: "'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,\n        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version",
    pad: '      ',
    sha256: 'b39700be849837f866f7d27017ed802dd749b11025040ceeb93c217ef3aa597f',
    binds: '[actor.holdingId, now, now, actor.holdingId, now]',
  },
  {
    file: 'executive-metric-projections.ts',
    full: `        SELECT b.holding_id id,'Legacy subscription' plan,b.status
        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)`,
    projection: "b.holding_id id,'Legacy subscription' plan,b.status",
    pad: '        ',
    sha256: 'ba48d05a609271a1ed54d7a48bb1287d27158b48b22c6a86a5a8e8536a6d2297',
    binds: '[a.holdingId, now, now, a.holdingId, now]',
  },
] as const;

/** Verbatim pre-r32 point reads now routed through the r29 point-read text. */
const POINTS = [
  {
    file: 'billing-jobs.ts',
    full: 'SELECT * FROM billing_accounts WHERE holding_id=$1',
    columns: '*', table: 'billing_accounts', suffix: 'holding_id=$1',
    sha256: '43d922b694527f900c0f047f99caed09e2a5e653efa16f5034ab72a28812c685',
    routed: "billingPointReadSql('*','billing_accounts','holding_id=$1')",
    uses: 2,
  },
  {
    file: 'billing-jobs.ts',
    full: 'SELECT customer_id FROM billing_accounts WHERE holding_id=$1',
    columns: 'customer_id', table: 'billing_accounts', suffix: 'holding_id=$1',
    sha256: '516cbff0ef8df5f6b7731dfa60f967f56eea17eb2e796f6c64eeb73d3aa7a867',
    routed: "billingPointReadSql('customer_id','billing_accounts','holding_id=$1')",
    uses: 2,
  },
  {
    file: 'billing-target.ts',
    full: 'SELECT payment_mode,consent_at,card_json FROM billing_accounts WHERE holding_id=$1',
    columns: 'payment_mode,consent_at,card_json', table: 'billing_accounts', suffix: 'holding_id=$1',
    sha256: '7ddea2b2b3ff981e588e34bfe063258f65f69444d10eee42001d546b508ee6f8',
    routed: "billingPointReadSql('payment_mode,consent_at,card_json','billing_accounts','holding_id=$1')",
    uses: 1,
  },
  {
    file: 'billing.ts',
    full: 'SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1',
    columns: '1', table: 'billing_target_sandbox_holdings', suffix: 'holding_id=$1',
    sha256: '69dd32a65214f1a4625ce47358d6f5a4764a1d6d6e81b9f2c259d4624230367e',
    routed: "billingPointReadSql('1','billing_target_sandbox_holdings','holding_id=$1')",
    uses: 1,
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

describe('billing read-tail r32 dedupe', () => {
  it('authors the legacy beta_billing head exactly once as read-only text', async () => {
    const helper = await src('billing-read-tail-r32-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(1);
    expect(helper.split('FROM beta_billing b WHERE').length - 1).toBe(1);
    expect(helper.split('billingLegacySubscriptionSql').length - 1).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes both legacy heads byte-for-byte with unchanged [holdingId, now] binds', () => {
    for (const oracle of LEGACY) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(billingLegacySubscriptionSql(oracle.projection, oracle.pad)).toBe(oracle.full);
      expect((oracle.full.match(/\?/g) ?? []).join(',')).toBe('?,?');
      expect(oracle.full).toContain('b.holding_id=?');
      expect(oracle.full).toContain('b.current_period_end*1000>?');
    }
  });

  it('routes both executive readers through the shared head with unchanged binds', async () => {
    for (const oracle of LEGACY) {
      const text = await src(oracle.file);
      expect(text).toContain("from './billing-read-tail-r32-sql'");
      expect(text.split('billingLegacySubscriptionSql(').length - 1).toBe(1);
      expect(text.split(oracle.full).length - 1).toBe(0);
      expect(text.split('beta_billing b WHERE').length - 1).toBe(0);
      expect(text).toContain(oracle.binds);
    }
  });

  it('recomposes all four point reads byte-for-byte through the r29 text', () => {
    for (const oracle of POINTS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(billingPointReadSql(oracle.columns, oracle.table, oracle.suffix)).toBe(oracle.full);
      expect((oracle.full.match(/\$\d/g) ?? []).join(',')).toBe('$1');
    }
  });

  it('removes the r15 account/customer constants and routes every point-read site', async () => {
    const r15 = await src('billing-read-tail-r15-sql.ts');
    expect(r15.split('SELECT').length - 1).toBe(0);
    expect(r15).not.toContain('"SELECT * FROM billing_invoices WHERE id=$1"');
    expect(r15).not.toContain('BILLING_ACCOUNT_BY_HOLDING_SQL');
    expect(r15).not.toContain('BILLING_CUSTOMER_BY_HOLDING_SQL');
    const jobs = await src('billing-jobs.ts');
    const target = await src('billing-target.ts');
    const billing = await src('billing.ts');
    for (const oracle of POINTS) {
      const text = oracle.file === 'billing-jobs.ts' ? jobs : oracle.file === 'billing-target.ts' ? target : billing;
      expect(text.split(oracle.full).length - 1).toBe(0);
      expect(text.split(oracle.routed).length - 1).toBe(oracle.uses);
    }
    expect(jobs).toContain("from './billing-read-tail-r29-sql'");
    expect(jobs.split('BILLING_INVOICE_BY_ID_SQL').length - 1).toBe(3);
    expect(jobs.split('billingPointReadSql(').length - 1).toBe(8);
    expect(target.split('billingPointReadSql(').length - 1).toBe(4);
    expect(billing.split('billingPointReadSql(').length - 1).toBe(9); // r35 effective-month guard + provider total
  });

  it('leaves FOR UPDATE, advisory and native owner reads untouched', async () => {
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(billing).toContain('nativeBillingOwnerQuery');
    expect(billing).not.toContain('billingLegacySubscriptionSql');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
    expect(billing).not.toContain('billing-read-tail-r25-sql');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=53 to 42', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(6);
    expect(bare).toBe(6);
    expect(spaced).toBeLessThan(48);
    expect(bare).toBeLessThan(48);
  });
});
