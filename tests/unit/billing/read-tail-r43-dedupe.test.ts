import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingLegacySubscriptionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r32-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r43/r32 legacy beta_billing renderings captured at BEFORE=6
 * (r32 renderer). SHA-256 values are byte-identity oracles only, never live
 * query sources. */
const LEGACY = [
  {
    name: 'evidence/legacy',
    full: "      SELECT 'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,\n        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version\n      FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')\n        AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)",
    projection: "'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,\n        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version",
    pad: '      ',
    len: 397,
    sha256: 'b39700be849837f866f7d27017ed802dd749b11025040ceeb93c217ef3aa597f',
    binds: '?,?',
    tail: "\n      FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')\n        AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)",
  },
  {
    name: 'metric/legacy',
    full: "        SELECT b.holding_id id,'Legacy subscription' plan,b.status\n        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')\n          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)",
    projection: "b.holding_id id,'Legacy subscription' plan,b.status",
    pad: '        ',
    len: 263,
    sha256: 'ba48d05a609271a1ed54d7a48bb1287d27158b48b22c6a86a5a8e8536a6d2297',
    binds: '?,?',
    tail: "\n        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')\n          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)",
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

describe('billing read-tail r43 tail-aware fold of the r32 beta_billing head', () => {
  it('turns the r32 renderer into a strict specialization of the r35 projection head', async () => {
    const legacy = await src('billing-read-tail-r32-sql.ts');
    const projection = await src('billing-read-tail-r35-sql.ts');
    expect(legacy.split('SELECT').length - 1).toBe(0);
    expect(legacy.split('billingProjectionSql(').length - 1).toBe(1);
    expect(legacy).toContain("import { billingProjectionSql } from './billing-read-tail-r35-sql';");
    expect(projection.split('SELECT').length - 1).toBe(1);
    expect(projection.split('billingProjectionSql').length - 1).toBe(1);
    expect((await src('billing-read-tail-r33-sql.ts')).split('SELECT').length - 1).toBe(0);
    for (const helper of [legacy, projection]) {
      expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('recomposes both r32 heads byte-for-byte through r35 with unchanged binds', () => {
    for (const oracle of LEGACY) {
      expect(sha256(oracle.full), oracle.name).toBe(oracle.sha256);
      expect(oracle.full.length, oracle.name).toBe(oracle.len);
      expect(billingLegacySubscriptionSql(oracle.projection, oracle.pad), oracle.name).toBe(oracle.full);
      expect(`${oracle.pad}${billingProjectionSql(oracle.projection, oracle.tail)}`, oracle.name).toBe(oracle.full);
      expect(billingProjectionSql(oracle.projection, oracle.tail), oracle.name).toBe(oracle.full.slice(oracle.pad.length));
      expect((oracle.full.match(/\?/g) ?? []).join(','), oracle.name).toBe(oracle.binds);
      expect((oracle.full.match(/\$\d/g) ?? []).join(','), oracle.name).toBe('');
      expect(billingProjectionSql(oracle.projection, oracle.tail), oracle.name).not.toContain(' \n');
    }
  });

  it('keeps the prior r35 byte shapes for every non-newline tail and omits the separator for newline tails', () => {
    for (const [columns, tail] of [
      ['name', 'FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1'],
      ['id', 'FROM billing_accounts'],
      ['*', 'FROM billing_receipts WHERE id=$1'],
    ] as const) {
      expect(billingProjectionSql(columns, tail)).toBe(`SELECT ${columns} ${tail}`);
      expect(billingProjectionSql(columns, tail)).not.toContain(' \n');
    }
    const newlineTail = '\n      FROM beta_billing b';
    expect(billingProjectionSql('id', newlineTail)).toBe(`SELECT id${newlineTail}`);
    expect(billingProjectionSql('id', newlineTail)).not.toContain(' \n');
  });

  it('leaves both executive callers routed through the unchanged r32 helper', async () => {
    for (const [file, binds] of [
      ['executive-main-evidence-sources.ts', '[actor.holdingId, now, now, actor.holdingId, now]'],
      ['executive-metric-projections.ts', '[a.holdingId, now, now, a.holdingId, now]'],
    ] as const) {
      const text = await src(file);
      expect(text, file).toContain("from './billing-read-tail-r32-sql'");
      expect(text.split('billingLegacySubscriptionSql(').length - 1, file).toBe(1);
      expect(text, file).toContain(binds);
    }
    const r32 = await src('billing-read-tail-r32-sql.ts');
    expect(r32.split("from './billing-read-tail-r35-sql'").length - 1).toBe(1);
  });

  it('leaves FOR UPDATE, advisory, native and try-lock reads untouched', async () => {
    const billing = await src('billing.ts');
    const jobs = await src('billing-jobs.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("SELECT pg_advisory_xact_lock(hashtextextended('billing-plans',0))");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=6 to 5', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(5);
    expect(bare).toBe(5);
    expect(spaced).toBeLessThan(6);
    expect(bare).toBeLessThan(6);
  });
});
