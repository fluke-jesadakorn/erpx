import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingUsageMeterSql, billingSeatUsageSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r20-sql';

const src = (name: string) => readFile(new URL(`../../../packages/server-billing/src/adapters/${name}`, import.meta.url), 'utf8');

/** Byte-identical originals, with the awaited native commercial source rendered as SRC. */
const METER_MEASURED = `SELECT source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until,active_at_period_end,snapshot_id,
      byte_milliseconds::text AS byte_milliseconds
     FROM (SRC) billing_usage_meters WHERE holding_id=$1 AND month=$2 ORDER BY source_key`;
const SEAT_MEASURED = `SELECT intervals_json,snapshot_id FROM (SRC) billing_seat_usage
     WHERE holding_id=$1 AND month=$2`;
const METER_RATING = `SELECT source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until
     FROM (SRC) billing_usage_meters WHERE holding_id=$1 AND month=$2 ORDER BY source_key`;
const SEAT_RATING = `SELECT user_key,intervals_json,snapshot_id FROM (SRC) billing_seat_usage WHERE holding_id=$1 AND month=$2 ORDER BY user_key`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r20 parameterized family dedupe', () => {
  it('renders both shared families byte-identically to the original inline SQL', () => {
    const meterMeasured = `source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until,active_at_period_end,snapshot_id,
      byte_milliseconds::text AS byte_milliseconds`;
    const meterRating = `source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until`;
    expect(billingUsageMeterSql(meterMeasured, 'SRC')).toBe(METER_MEASURED);
    expect(billingSeatUsageSql('intervals_json,snapshot_id', 'SRC', `
     WHERE holding_id=$1 AND month=$2`)).toBe(SEAT_MEASURED);
    expect(billingUsageMeterSql(meterRating, 'SRC')).toBe(METER_RATING);
    expect(billingSeatUsageSql('user_key,intervals_json,snapshot_id', 'SRC', ' WHERE holding_id=$1 AND month=$2 ORDER BY user_key')).toBe(SEAT_RATING);
  });

  it('routes every family call site through the shared helpers with no residual inline SQL', async () => {
    const target = await src('billing-target.ts');
    expect(target.split("from './billing-read-tail-r20-sql'").length - 1).toBe(1);
    expect(target.split('billingUsageMeterSql(').length - 1).toBe(2);
    expect(target.split('billingSeatUsageSql(').length - 1).toBe(2);
    for (const inline of [
      '`SELECT source_key,meter_kind,company_id,branch_id,funding_source,quantity,',
      '`SELECT intervals_json,snapshot_id FROM (',
      '`SELECT user_key,intervals_json,snapshot_id FROM (',
    ]) expect(target).not.toContain(inline);
    expect(target).toContain('await billingUsageMeterSql(`source_key,meter_kind');
    expect(target).toContain("await billingSeatUsageSql('intervals_json,snapshot_id'");
    expect(target).toContain("await billingSeatUsageSql('user_key,intervals_json,snapshot_id'");
  });

  it('keeps the helpers read-only and leaves advisory/FOR UPDATE reads untouched', async () => {
    for (const fragment of [METER_MEASURED, SEAT_MEASURED, METER_RATING, SEAT_RATING]) {
      expect(fragment).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const billing = await src('billing.ts');
    expect(billing).toContain('"SELECT pg_advisory_xact_lock(hashtextextended($1,0))"');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    const jobs = await src('billing-jobs.ts');
    expect(jobs).toContain('"SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked"');
  });

  it('lowers the billing owner SELECT KPI strictly below the recorded BEFORE=71 baseline', async () => {
    const files = await tsFiles(new URL('../../../packages/server-billing/src/', import.meta.url));
    let count = 0;
    for (const file of files) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    expect(count).toBeLessThan(71);
    expect(count).toBe(69);
  });
});
