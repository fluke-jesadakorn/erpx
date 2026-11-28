import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { BILLING_PLAN_NAME_LATERAL_SQL } from '../../../modules/billing/server/src/adapters/billing-read-tail-r18-sql';
import { billingEnrollmentSubscriptionSql } from '../../../modules/billing/server/src/adapters/billing-read-tail-r33-sql';
import { billingProjectionSql } from '../../../modules/billing/server/src/adapters/billing-read-tail-r35-sql';

const srcRoot = new URL('../../../modules/billing/server/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r42 r33 renderings captured at BEFORE=7 (HEAD 6d08598b3). The
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const PLAN_TAIL = 'FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1';
const LATERAL = 'LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE';
const HEADS = [
  {
    name: 'enrollment/evidence',
    full: "      SELECT 'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,\n        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,\n        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version\n      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id\n      LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE\n      WHERE a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)",
    sha256: '78959199cfc195d6d8163ffe8cb7cc331772d578df28287b3b3f53c0434c0461',
    binds: '?,?,?',
    columns: "'enrollment'::text source_kind,e.id record_id,COALESCE(p.name,e.plan_id) plan_name,\n        CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status,e.started_at,\n        CASE WHEN e.pending_end THEN e.effective_at ELSE NULL END effective_at,a.version record_version",
    from: '      FROM billing_accounts a JOIN billing_enrollments e ON e.holding_id=a.holding_id',
    holding: 'a.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)',
    pad: '      ',
  },
  {
    name: 'enrollment/metric',
    full: "        SELECT e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status\n        FROM billing_enrollments e\n        LEFT JOIN LATERAL (SELECT name FROM billing_plans WHERE id=e.plan_id ORDER BY revision DESC LIMIT 1) p ON TRUE\n        WHERE e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL\n          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)",
    sha256: '6bcd54a6afc84e2dd94c0c07e95d13e5eceacf252f8ae48863ab5bf97bacd5ad',
    binds: '?,?,?',
    columns: "e.id,COALESCE(p.name,e.plan_id) plan,CASE WHEN e.pending_end THEN 'ending' ELSE 'active' END status",
    from: '        FROM billing_enrollments e',
    holding: "e.holding_id=? AND e.started_at<=? AND e.ended_at IS NULL\n          AND (e.pending_end=FALSE OR e.effective_at IS NULL OR e.effective_at>?)",
    pad: '        ',
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

describe('billing read-tail r42 tail-aware fold of the r33 enrollment head', () => {
  it('turns the r33 head into a strict specialization of the tail-aware r35 projection head', async () => {
    const enrollment = await src('billing-read-tail-r33-sql.ts');
    const projection = await src('billing-read-tail-r35-sql.ts');
    expect(enrollment.split('SELECT').length - 1).toBe(0);
    expect(enrollment.split('billingProjectionSql(').length - 1).toBe(1);
    expect(enrollment).toContain("import { billingProjectionSql } from './billing-read-tail-r35-sql';");
    expect(enrollment.split('${BILLING_PLAN_NAME_LATERAL_SQL}').length - 1).toBe(1);
    expect(projection.split('SELECT').length - 1).toBe(1);
    expect(projection.split('billingProjectionSql').length - 1).toBe(1);
    expect(projection).toContain("tail.startsWith('\\n') ? '' : ' '");
    expect((await src('billing-read-tail-r32-sql.ts')).split('SELECT').length - 1).toBe(0);
    for (const helper of [enrollment, projection]) {
      expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('recomposes the lateral and both r33 heads byte-for-byte with unchanged binds', () => {
    expect(sha256(LATERAL)).toBe('35daa6dd76558861b0f7a1ec3800ab4967bdd21051d5a41701e4579413defbdc');
    expect(billingProjectionSql('name', PLAN_TAIL)).toBe(`SELECT name ${PLAN_TAIL}`);
    expect(`LEFT JOIN LATERAL (${billingProjectionSql('name', PLAN_TAIL)}) p ON TRUE`).toBe(LATERAL);
    expect(BILLING_PLAN_NAME_LATERAL_SQL).toBe(LATERAL);
    for (const head of HEADS) {
      const tail = `\n${head.from}\n${head.pad}${LATERAL}\n${head.pad}WHERE ${head.holding}`;
      expect(sha256(head.full), head.name).toBe(head.sha256);
      expect(billingEnrollmentSubscriptionSql(head.columns, head.from, head.holding, head.pad), head.name).toBe(head.full);
      expect(billingProjectionSql(head.columns, tail), head.name).toBe(head.full.slice(head.pad.length));
      expect(`${head.pad}${billingProjectionSql(head.columns, tail)}`, head.name).toBe(head.full);
      expect((head.full.match(/\?/g) ?? []).join(','), head.name).toBe(head.binds);
      expect((head.full.match(/\$\d/g) ?? []).join(','), head.name).toBe('');
      expect(head.full.includes(LATERAL), head.name).toBe(true);
    }
  });

  it('keeps the old byte shape for every non-newline tail and omits the separator for newline tails', () => {
    const tails = [
      ['r18', 'name', PLAN_TAIL],
      ['r22', `md5(COALESCE(string_agg(jsonb_build_array(e.id)::text,',' ORDER BY e.id),''))`, 'FROM billing_enrollments e WHERE e.holding_id=a.holding_id'],
      ['r27', `label label,count(*)::text value`, 'FROM '],
      ['r29', '*', 'FROM billing_receipts WHERE id=$1'],
      ['r30', 'id', 'FROM billing_accounts'],
    ] as const;
    for (const [name, columns, tail] of tails) {
      expect(billingProjectionSql(columns, tail), name).toBe(`SELECT ${columns} ${tail}`);
      expect(billingProjectionSql(columns, tail), name).not.toContain(' \n');
    }
    const newlineTail = '\n      FROM beta_billing b';
    expect(billingProjectionSql('id', newlineTail)).toBe(`SELECT id${newlineTail}`);
    expect(billingProjectionSql('id', newlineTail)).not.toContain(' \n');
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
    // Native lane 38 wave 2: the other caller carries no SQL; the byte-identical
    // head is pinned in the owner crate instead.
    const evidence = await src('executive-main-evidence-sources.ts');
    expect(evidence).toContain("from './native-lane-38/client'");
    expect(evidence).not.toContain(LATERAL);
    const rust = await src('native_lane_38/postgres.rs');
    expect(rust).toContain(LATERAL);
    const r33 = await src('billing-read-tail-r33-sql.ts');
    expect(r33.split("from './billing-read-tail-r18-sql'").length - 1).toBe(1);
    expect(r33.split('${BILLING_PLAN_NAME_LATERAL_SQL}').length - 1).toBe(1);
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

  it('lowers the billing owner SELECT KPI from BEFORE=7 to 6', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(5);
    expect(bare).toBe(5);
    expect(spaced).toBeLessThan(7);
    expect(bare).toBeLessThan(7);
  });
});
