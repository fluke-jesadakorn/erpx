import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingProviderCostsSql, billingUsageSnapshotSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r19-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-billing/src/adapters/${name}`, import.meta.url), 'utf8');

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r19 parameterized family dedupe', () => {
  it('renders the two shared SQL families with byte-identical text', () => {
    expect(billingProviderCostsSql()).toBe('SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2');
    expect(billingProviderCostsSql(' AND source_key=$3')).toBe('SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND source_key=$3');
    expect(billingUsageSnapshotSql('*', 'SRC')).toBe('SELECT * FROM (SRC) billing_usage_snapshots WHERE holding_id=$1 AND month=$2');
    expect(billingUsageSnapshotSql('active_seat_count,active_seat_observed_at', 'SRC')).toBe('SELECT active_seat_count,active_seat_observed_at FROM (SRC) billing_usage_snapshots WHERE holding_id=$1 AND month=$2');
  });

  it('routes every family call site through the shared helpers with no residual literal SQL', async () => {
    const billing = await src('billing.ts');
    expect(billing.split('billingProviderCostsSql(" AND source_key=$3")').length - 1).toBe(0); // native provider-cost cut retired the existingProviderCost call site; the helper remains owned by billing-target.ts
    expect(billing).not.toContain('"SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND source_key=$3"');

    const target = await src('billing-target.ts');
    expect(target.split('billingProviderCostsSql()').length - 1).toBe(1);
    expect(target.split('billingUsageSnapshotSql(').length - 1).toBe(3);
    expect(target).not.toContain('"SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2"');
    expect(target).not.toContain('`SELECT snapshot_id,refreshed_at,source_count');
    expect(target).not.toContain('`SELECT active_seat_count,active_seat_observed_at FROM (');
    expect(target).not.toContain('`SELECT * FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)})');
  });

  it('keeps the helpers read-only and leaves advisory/FOR UPDATE reads untouched', async () => {
    for (const fragment of [billingProviderCostsSql(), billingUsageSnapshotSql('*', 'SRC')]) {
      expect(fragment).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const billing = await src('billing.ts');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain('"SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked"');
  });

  it('lowers the billing owner SELECT KPI strictly below the r18 after=74 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(74);
  });
});
