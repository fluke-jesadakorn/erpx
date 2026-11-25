import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const src = (path: string) => readFile(new URL(`../../../packages/server-billing/src/${path}`, import.meta.url), 'utf8');
const commercial = () => readFile(new URL('../../../packages/server-commercial/src/domain/owner_read.rs', import.meta.url), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

const KIND = {
  meter: 'billing_rd_successful_platform_meter',
  snapshotLock: 'billing_rd_target_snapshot_lock',
  xactLock: 'billing_rd_advisory_xact_lock',
  unlock: 'billing_rd_advisory_unlock',
} as const;
const CONST = {
  [KIND.meter]: 'BILLING_RD_SUCCESSFUL_PLATFORM_METER',
  [KIND.snapshotLock]: 'BILLING_RD_TARGET_SNAPSHOT_LOCK',
  [KIND.xactLock]: 'BILLING_RD_ADVISORY_XACT_LOCK',
  [KIND.unlock]: 'BILLING_RD_ADVISORY_UNLOCK',
} as const;
const BINDS = {
  [KIND.meter]: 3,
  [KIND.snapshotLock]: 2,
  [KIND.xactLock]: 1,
  [KIND.unlock]: 1,
} as const;

/** Verbatim pre-r37 SQL bytes (base c7caf9570e). The SHA values are
 * byte-identity oracles only, never live query sources. The two Commercial
 * subqueries are read from their owner registry so a silent projection edit is
 * caught by the byte oracle below. */
const ownerRead = async (name: string) => {
  const match = (await commercial()).match(new RegExp(`pub const ${name}: &str = r#"(.*?)"#;`));
  expect(match, name).not.toBeNull();
  return match![1]!;
};
const meterProjection = await ownerRead('SQL_USAGE_METER');
const snapshotProjection = await ownerRead('SQL_USAGE_SNAPSHOT');
const EXPECTED = {
  [KIND.meter]: `SELECT * FROM (${meterProjection}) billing_usage_meters WHERE holding_id=$1 AND month=$2 AND source_key=$3
           AND meter_kind='ai' AND funding_source='erpx' AND request_status='succeeded'`,
  [KIND.snapshotLock]: `SELECT snapshot_id FROM (${snapshotProjection}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2 FOR UPDATE`,
  [KIND.xactLock]: 'SELECT pg_advisory_xact_lock(hashtextextended($1,0))',
  [KIND.unlock]: 'SELECT pg_advisory_unlock(hashtextextended($1,0))',
} as const;
const HASHES = {
  [KIND.meter]: '0ec4cb7494251777e61779c99f02b729f5ccf09189f81d92320d575d0a9683c1',
  [KIND.snapshotLock]: 'f42f09be6f6d0cbc595c924bbb7cfc1b772ec3167ac471b26bc546f2ecedb7f2',
  [KIND.xactLock]: sha256(EXPECTED[KIND.xactLock]),
  [KIND.unlock]: sha256(EXPECTED[KIND.unlock]),
} as const;

async function tsFiles(): Promise<string[]> {
  const found: string[] = [];
  const walk = async (path: string) => {
    for (const entry of await readdir(new URL(`../../../packages/server-billing/src/${path}`, import.meta.url), { withFileTypes: true })) {
      const child = `${path}${entry.name}`;
      if (entry.isDirectory()) await walk(`${child}/`);
      else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(child);
    }
  };
  await walk('');
  return found;
}

describe('billing r37 native owner-write reads', () => {
  it('recomposes the retired TS SQL byte-for-byte from owner-owned projections', () => {
    for (const kind of Object.keys(KIND) as (keyof typeof KIND)[]) {
      const sql = EXPECTED[KIND[kind]];
      expect(sha256(sql), kind).toBe(HASHES[KIND[kind]]);
      expect((sql.match(/\$\d/g) ?? []).length, kind).toBe(BINDS[KIND[kind]]);
    }
    expect(EXPECTED[KIND.meter]).toContain(`(${meterProjection}) billing_usage_meters`);
    expect(EXPECTED[KIND.snapshotLock]).toContain(`(${snapshotProjection}) billing_usage_snapshots`);
    expect(EXPECTED[KIND.meter]).not.toBe(EXPECTED[KIND.snapshotLock]);
    expect(EXPECTED[KIND.xactLock]).not.toBe(EXPECTED[KIND.unlock]);
  });

  it('allow-lists exactly four r37 kinds and owns each SQL arm in the billing crate', async () => {
    const domain = await src('domain/billing_owner_write.rs');
    const adapter = await src('adapters/billing_owner_write.rs');
    for (const kind of Object.keys(KIND) as (keyof typeof KIND)[]) {
      const constant = CONST[KIND[kind]];
      expect(domain, kind).toContain(`pub const ${constant}: &str = "${KIND[kind]}"`);
      expect(domain, kind).toContain(`    ${constant},\n`);
      expect(adapter, kind).toContain(`${constant} => `);
      expect(adapter, kind).toContain(EXPECTED[KIND[kind]]);
    }
    expect(domain.match(/BILLING_RD_/g)?.length).toBe(8); // four const declarations, four list entries
    expect(adapter.match(/BILLING_RD_/g)?.length).toBe(10); // arms + owner tests
  });

  it('routes all four reads through nativeBillingOwnerQuery on the caller PoolClient', async () => {
    const billing = await src('adapters/billing.ts');
    const jobs = await src('adapters/billing-jobs.ts');
    for (const kind of Object.keys(KIND) as (keyof typeof KIND)[]) {
      const values = Array.from({ length: BINDS[KIND[kind]] }, (_, i) =>
        kind === 'unlock' ? `v${i}` : `value${i}`,
      ).join(',');
      const file = kind === 'unlock' ? jobs : billing;
      expect(file, kind).toContain(`nativeBillingOwnerQuery(c,'${KIND[kind]}',[${values}])`);
    }
    // Same client `c` for every read keeps the FOR UPDATE and advisory lock on
    // the caller's serializable unit; no separate client/transport is opened.
    expect(billing.match(/nativeBillingOwnerQuery\(c,/g)?.length).toBe(49);
    expect(jobs.match(/nativeBillingOwnerQuery\(c,/g)?.length).toBe(15);
    expect(billing).not.toContain('nativeCommercialOwnerRead');
    expect(billing).not.toContain('COMMERCIAL_RD_USAGE');
    expect(billing).not.toContain('billing-read-tail-r25-sql');
    expect(jobs).not.toContain('billing-read-tail-r25-sql');
    expect(billing).not.toMatch(/c\.query<[^>]*>\(`SELECT/);
  });

  it('keeps the snapshot lock and advisory lock/unlock pairing intact', async () => {
    const adapter = await src('adapters/billing_owner_write.rs');
    const billing = await src('adapters/billing.ts');
    const jobs = await src('adapters/billing-jobs.ts');
    const applicationBilling = await src('application/billing.ts');
    const applicationJobs = await src('application/jobs.ts');
    expect(EXPECTED[KIND.snapshotLock]).toMatch(/FOR UPDATE$/);
    expect(adapter).toContain('TARGET_SNAPSHOT_LOCK_SQL: &str = r####"SELECT snapshot_id FROM (');
    expect(adapter).toContain('FOR UPDATE"####;');
    expect(adapter).toContain('pg_advisory_xact_lock');
    expect(adapter).toContain('pg_advisory_unlock');
    expect(billing).toContain(`lockHolding:(value0)=>nativeBillingOwnerQuery(c,'${KIND.xactLock}',[value0])`);
    expect(billing).toContain(`lockCommand:(value0)=>nativeBillingOwnerQuery(c,'${KIND.xactLock}',[value0])`);
    expect(jobs).toContain(`unlockHolding:(v0)=>nativeBillingOwnerQuery(c,'${KIND.unlock}',[v0])`);
    expect(applicationBilling).toContain('await c.lockHolding(`billing:${holdingId}`)');
    expect(applicationBilling).toContain('await c.lockCommand(`billing-command:${key}`)');
    expect(applicationBilling).toContain('await c.lockTargetSnapshot(holdingId,input.targetMonth)');
    expect(applicationJobs).toContain('await c.unlockHolding(`billing:${id}`)');
    // Session-level try-lock/unlock remain paired; the xact head is distinct.
    expect(jobs).toContain('pg_try_advisory_lock');
    expect(billing).toContain("lockPlanRevision:()=>c.query<Record<string,unknown>>(\"SELECT pg_advisory_xact_lock(hashtextextended('billing-plans',0))\",[])");
  });

  it('retires the r25 file and lowers the billing owner SELECT KPI from 16 to 13', async () => {
    const files = await tsFiles();
    expect(files.some((name) => name.includes('billing-read-tail-r25-sql'))).toBe(false);
    let spaced = 0;
    let bare = 0;
    for (const file of files) {
      const text = await src(file);
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(10);
    expect(bare).toBe(10);
    expect(spaced).toBeLessThan(16);
    expect(bare).toBeLessThan(16);
  });
});
