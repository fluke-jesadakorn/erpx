import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';
import { billingWatermarkRevisionSql as revision } from '../../../packages/server-billing/src/adapters/billing-read-tail-r22-sql';
import { billingProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r35-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r40 watermark revisions captured at BEFORE=9 (base 43d25492fa).
 * The SHA-256 values are byte-identity oracles only, never live query sources. */
const REVISIONS = [
  {
    name: 'enrollment',
    fields: 'e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at',
    orderBy: 'e.id', from: 'billing_enrollments e', filter: 'e.holding_id=a.holding_id',
    full: `(SELECT md5(COALESCE(string_agg(jsonb_build_array(e.id,e.plan_id,e.started_at,e.ended_at,e.pending_end,e.pending_plan,e.effective_at)::text,',' ORDER BY e.id),'')) FROM billing_enrollments e WHERE e.holding_id=a.holding_id)`,
    sha256: 'faa343da7527c3966ae914a4344c198e7dc1c19afd9a088dd1e1d5aaef2d6c5b',
  },
  {
    name: 'invoice',
    fields: 'i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind',
    orderBy: 'i.id', from: 'billing_invoices i', filter: 'i.holding_id=a.holding_id',
    full: `(SELECT md5(COALESCE(string_agg(jsonb_build_array(i.id,i.status,i.due_at,i.total_satang,i.updated_at,i.kind)::text,',' ORDER BY i.id),'')) FROM billing_invoices i WHERE i.holding_id=a.holding_id)`,
    sha256: '74c4362bcabe689604f8778c3123971c54395ada196b4beae1f3f351ffc8653d',
  },
  {
    name: 'legacy',
    fields: 'b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at',
    orderBy: 'b.holding_id', from: 'beta_billing b', filter: 'b.holding_id=a.holding_id',
    full: `(SELECT md5(COALESCE(string_agg(jsonb_build_array(b.subscription_id,b.status,b.current_period_end,b.last_event_at,b.updated_at)::text,',' ORDER BY b.holding_id),'')) FROM beta_billing b WHERE b.holding_id=a.holding_id)`,
    sha256: 'f9373a4f391726f5e16c808b3db69ed93252682076613fe96ec084757b92f11f',
  },
  {
    name: 'plan',
    fields: 'p.id,p.revision,p.name,p.effective_month',
    orderBy: 'p.id,p.revision', from: 'billing_plans p',
    filter: 'p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id)',
    full: `(SELECT md5(COALESCE(string_agg(jsonb_build_array(p.id,p.revision,p.name,p.effective_month)::text,',' ORDER BY p.id,p.revision),'')) FROM billing_plans p WHERE p.id IN (SELECT e.plan_id FROM billing_enrollments e WHERE e.holding_id=a.holding_id))`,
    sha256: 'ae00898f8a911e77362f465a9fbab6d7e8b2cb6e38e19710b67299141553c809',
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

describe('billing read-tail r40 dedupe of the r22 watermark revision head', () => {
  it('turns the r22 head into a strict specialization of the r35 projection head', async () => {
    const watermark = await src('billing-read-tail-r22-sql.ts');
    const projection = await src('billing-read-tail-r35-sql.ts');
    expect(watermark.split('SELECT').length - 1).toBe(0);
    expect(watermark.split('billingProjectionSql(').length - 1).toBe(1);
    expect(watermark).toContain("import { billingProjectionSql } from './billing-read-tail-r35-sql';");
    expect(watermark).toContain('billingProjectionSql(');
    expect(watermark).not.toContain('SELECT md5(');
    expect(projection.split('SELECT').length - 1).toBe(1);
    expect(projection.split('billingProjectionSql').length - 1).toBe(1);
    for (const helper of [watermark, projection]) {
      expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('recomposes all four watermark revisions byte-for-byte through the r35 head', () => {
    for (const r of REVISIONS) {
      const columns = `md5(COALESCE(string_agg(jsonb_build_array(${r.fields})::text,',' ORDER BY ${r.orderBy}),''))`;
      const tail = `FROM ${r.from} WHERE ${r.filter}`;
      expect(sha256(r.full), r.name).toBe(r.sha256);
      expect(billingProjectionSql(columns, tail), r.name).toBe(r.full.slice(1, -1));
      expect(`(${billingProjectionSql(columns, tail)})`, r.name).toBe(r.full);
      expect(revision(r.fields, r.orderBy, r.from, r.filter), r.name).toBe(r.full);
      expect((r.full.match(/\$\d/g) ?? []).join(','), r.name).toBe('');
    }
  });

  it('leaves every watermark call site routed through the unchanged helper', async () => {
    const watermarks = await src('executive-metric-watermarks.ts');
    expect(watermarks.split("from './billing-read-tail-r22-sql'").length - 1).toBe(1);
    expect(watermarks.split('revision(').length - 1).toBe(4);
    for (const r of REVISIONS) {
      expect(watermarks, r.name).not.toContain(`(SELECT md5(COALESCE(string_agg(jsonb_build_array(${r.fields.split(',')[0]},`);
    }
  });

  it('leaves FOR UPDATE, advisory, native and try-lock reads untouched', async () => {
    const billing = await src('billing.ts');
    const jobs = await src('billing-jobs.ts');
    expect(billing).toContain('"SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE"');
    expect(billing).toContain('"SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE"');
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_target_snapshot_lock',[value0,value1])");
    expect(billing).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_xact_lock',[value0])");
    expect(billing).toContain('nativeBillingOwnerQuery');
    expect(jobs).toContain("nativeBillingOwnerQuery(c,'billing_rd_advisory_unlock',[v0])");
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(jobs).toContain('nativeBillingOwnerQuery');
  });

  it('lowers the billing owner SELECT KPI from BEFORE=9 to 8', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(7);
    expect(bare).toBe(7);
    expect(spaced).toBeLessThan(9);
    expect(bare).toBeLessThan(9);
  });
});
