import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { BILLING_PLAN_NAME_LATERAL_SQL } from '../../../packages/server-billing/src/adapters/billing-read-tail-r18-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';
import { billingEnrollmentSubscriptionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r33-sql';
import { billingLegacySubscriptionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r32-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r41 fragments captured at BEFORE=8 (HEAD a348ca7b1).
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const LATERAL = 'LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE';
const PLAN_TAIL = 'FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1';
const HEADS = [
  {
    name: 'enrollment/evidence',
    full: `      SELECT 'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,
        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,
        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version
      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id
      LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
      WHERE a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)`,
    sha256: '78959199cfc195d6d8163ffe8cb7cc331772d578df28287b3b3f53c0434c0461',
    binds: '?,?,?',
    actual: () => billingEnrollmentSubscriptionSql(
      "'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,\n        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,\n        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version",
      '      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id',
      'a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)',
      '      ',
    ),
  },
  {
    name: 'enrollment/metric',
    full: `        SELECT e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status
        FROM billing_enrollments e
        LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE
        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL
          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)`,
    sha256: '6bcd54a6afc84e2dd94c0c07e95d13e5eceacf252f8ae48863ab5bf97bacd5ad',
    binds: '?,?,?',
    actual: () => billingEnrollmentSubscriptionSql(
      "e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status",
      '        FROM billing_enrollments e',
      "e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL\n          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)",
      '        ',
    ),
  },
  {
    name: 'legacy/evidence',
    full: `      SELECT 'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,
        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version
      FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
        AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)`,
    sha256: 'b39700be849837f866f7d27017ed802dd749b11025040ceeb93c217ef3aa597f',
    binds: '?,?',
    actual: () => billingLegacySubscriptionSql(
      "'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status,\n        NULL::bigint started_at,b.current_period_end effective_at,b.updated_at record_version",
      '      ',
    ),
  },
  {
    name: 'legacy/metric',
    full: `        SELECT b.holding_id id,'Legacy subscription' plan,b.status
        FROM beta_billing b WHERE b.holding_id=? AND b.subscription_id IS NOT NULL AND b.status IN ('active','trialing')
          AND (b.current_period_end IS NULL OR b.current_period_end*1000>?)`,
    sha256: 'ba48d05a609271a1ed54d7a48bb1287d27158b48b22c6a86a5a8e8536a6d2297',
    binds: '?,?',
    actual: () => billingLegacySubscriptionSql("b.holding_id id,'Legacy subscription' plan,b.status", '        '),
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

describe('billing read-tail r41 dedupe of the r18 plan-name lateral', () => {
  it('turns the r18 lateral head into a strict specialization of the r35 projection head', async () => {
    const lateral = await src('billing-read-tail-r18-sql.ts');
    expect(lateral.split('SELECT').length - 1).toBe(0);
    expect(lateral.split('billingProjectionSql(').length - 1).toBe(1);
    expect(lateral).toContain("import { billingProjectionSql } from './billing-read-tail-r35-sql';");
    const projection = await src('billing-read-tail-r35-sql.ts');
    expect(projection.split('SELECT').length - 1).toBe(1);
    expect(projection.split('billingProjectionSql').length - 1).toBe(1);
    for (const helper of [lateral, projection]) {
      expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('recomposes the lateral byte-for-byte through the shared head', () => {
    expect(sha256(LATERAL)).toBe('35daa6dd76558861b0f7a1ec3800ab4967bdd21051d5a41701e4579413defbdc');
    expect(billingProjectionSql('name', PLAN_TAIL)).toBe(`SELECT name ${PLAN_TAIL}`);
    expect(`LEFT JOIN LATERAL (${billingProjectionSql('name', PLAN_TAIL)}) p ON TRUE`).toBe(LATERAL);
    expect(BILLING_PLAN_NAME_LATERAL_SQL).toBe(LATERAL);
    expect((LATERAL.match(/\$\d/g) ?? []).join(',')).toBe('');
    expect(LATERAL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes all four subscription heads byte-for-byte with unchanged binds', () => {
    for (const head of HEADS) {
      expect(sha256(head.full), head.name).toBe(head.sha256);
      expect(head.actual(), head.name).toBe(head.full);
      expect(head.full.includes(LATERAL), head.name).toBe(head.name.startsWith('enrollment'));
      expect((head.full.match(/\?/g) ?? []).join(','), head.name).toBe(head.binds);
      expect((head.full.match(/\$\d/g) ?? []).join(','), head.name).toBe('');
    }
  });

  it('leaves every executive caller routed through the unchanged r32/r33 helpers', async () => {
    const text = await src('executive-metric-projections.ts');
    expect(text).toContain("from './billing-read-tail-r33-sql'");
    expect(text).toContain("from './billing-read-tail-r32-sql'");
    expect(text.split('billingEnrollmentSubscriptionSql(').length - 1).toBe(1);
    expect(text.split('billingLegacySubscriptionSql(').length - 1).toBe(1);
    expect(text).not.toContain(LATERAL);
    expect(text).not.toContain("from './billing-read-tail-r18-sql'");
    expect(text).toContain('[a.holdingId, now, now, a.holdingId, now]');
    // Native lane 38 wave 2: the other caller carries no SQL; the lateral is
    // pinned in the owner crate alongside the byte-identical subscription head.
    const evidence = await src('executive-main-evidence-sources.ts');
    expect(evidence).toContain("from './native-lane-38/client'");
    expect(evidence).not.toContain(LATERAL);
    const rust = await src('native_lane_38/postgres.rs');
    expect(rust).toContain(LATERAL);
    expect(rust).toContain("'legacy'::text source_kind,b.holding_id record_id,'Legacy subscription'::text plan_name,b.status");
    const r33 = await src('billing-read-tail-r33-sql.ts');
    expect(r33.split("from './billing-read-tail-r18-sql'").length - 1).toBe(1);
    expect(r33.split('${BILLING_PLAN_NAME_LATERAL_SQL}').length - 1).toBe(1);
    expect(r33.split('SELECT').length - 1).toBe(0);
    expect(r33.split('billingProjectionSql(').length - 1).toBe(1);
    expect(r33).toContain("import { billingProjectionSql } from './billing-read-tail-r35-sql';");
    expect((await src('billing-read-tail-r32-sql.ts')).split('SELECT').length - 1).toBe(0);
  });

  it('leaves FOR UPDATE, advisory, native and try-lock reads untouched', async () => {
    const billing = await src('billing.ts');
    const jobs = await src('billing-jobs.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(\'billing-plans\',0))"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=8 to 7', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(5);
    expect(bare).toBe(5);
    expect(spaced).toBeLessThan(8);
    expect(bare).toBeLessThan(8);
  });
});
