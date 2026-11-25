import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

const adapters = new URL('../../../packages/server-maintenance/src/adapters/', import.meta.url);
const src = (name: string) => readFile(new URL(name, adapters), 'utf8');
const srcRoot = resolve(__dirname, '../../../packages/server-maintenance/src');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

/** Pre-r30 byte oracles: the Work/Plan source-image projection and the scoped
 * source CTE in package-obligations shared the byte-identical row head
 * `SELECT ${alias}.*` (rendered `SELECT work.*` / `SELECT plan.*`). r30 keeps
 * every composed string byte-identical while delegating that head to one
 * owner-local aliasRowsSql renderer. Binds stay in the callers. */
const headWork = 'SELECT work.*';
const headPlan = 'SELECT plan.*';
const scopedWork = 'SELECT work.* FROM (<WORK>) work,scope WHERE work.company_id=scope.company_id AND <EXTRA>';
const scopedPlan = "SELECT plan.* FROM (<PLANS>) plan,scope WHERE plan.company_id=scope.company_id AND scope.package_id<>'assets-parts-costs'";
const ownerTail = (table: string, alias: string, assetsSql: string) =>
  `,COALESCE(asset.source_valid AND asset.branch_id=${alias}.branch_id AND asset.department_id IS NOT DISTINCT FROM ${alias}.department_id,false) source_valid FROM ${table} ${alias} LEFT JOIN (${assetsSql}) asset ON asset.id=${alias}.asset_id AND asset.company_id=${alias}.company_id`;

describe('maintenance read-tail r30 alias-row head delegation', () => {
  it('pins the byte-identical head oracles', () => {
    expect(sha(headWork)).toBe('3e65e16a1a6202c05220c7939fb6a3ff19d3d334e71038d89410f0268c06d4b8');
    expect(sha(headPlan)).toBe('473470c3cedbffcb3f8488913194b9c3c9198622cd2e72d3932467a0eedaa0f9');
    expect(sha(scopedWork)).toBe('35ec5767174cf08341a40d46e27db8d902b6b1bdca2ac080974d2d063446875b');
    expect(sha(scopedPlan)).toBe('668e7a89e0739ec790359c7590cc03885adedc8e505ab2dc5ed7a876d943b7bd');
  });

  it('renders all four callers byte-identically through aliasRowsSql', async () => {
    const { aliasRowsSql } = await import('../../../packages/server-maintenance/src/adapters/read-tail-r30-alias-rows');
    const { maintenancePackageSettlementSources, scopedSourceCte } =
      await import('../../../packages/server-maintenance/src/adapters/package-obligations');
    const { assetPackageSettlementSources: assets } =
      await import('../../../packages/server-assets/src/public/package-obligations');

    expect(aliasRowsSql('work', '')).toBe(headWork);
    expect(aliasRowsSql('plan', '')).toBe(headPlan);

    const work = aliasRowsSql('work', ownerTail('maintenance_work_orders', 'work', assets.assets));
    const plans = aliasRowsSql('plan', ownerTail('maintenance_preventive_plans', 'plan', assets.assets));
    expect(maintenancePackageSettlementSources.work).toBe(work);
    expect(maintenancePackageSettlementSources.plans).toBe(plans);
    expect(sha(maintenancePackageSettlementSources.work)).toBe('f4345473af8c4a40895e707b37b7aabd72e97335f8aa7f0fc44616e6bed77684');
    expect(sha(maintenancePackageSettlementSources.plans)).toBe('d79081edd19928e152c075010ea96f95518d77aac4840a5e54201cde66699d83');

    expect(scopedSourceCte('work', '<WORK>', '<EXTRA>')).toBe(scopedWork);
    expect(scopedSourceCte('plan', '<PLANS>', "scope.package_id<>'assets-parts-costs'")).toBe(scopedPlan);

    for (const sql of [headWork, headPlan, work, plans, scopedWork, scopedPlan]) {
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  it('keeps one head definition, both callers and no inline alias-qualified head', async () => {
    const helper = await src('read-tail-r30-alias-rows.ts');
    const pkg = await src('package-obligations.ts');
    expect(helper.match(/SELECT/g)).toHaveLength(1);
    expect(helper).toContain('export const aliasRowsSql = (alias: string, tail: string): string => `SELECT ${alias}.*${tail}`;');
    expect(pkg).toContain('import {aliasRowsSql} from "./read-tail-r30-alias-rows"');
    expect(pkg).toContain('aliasRowsSql(alias,`,COALESCE(asset.source_valid');
    expect(pkg).toContain('export const scopedSourceCte=(alias:"work"|"plan",source:string,extra:string)=>aliasRowsSql(alias,` FROM (${source})');
    expect(pkg).not.toContain('`SELECT ${alias}.*,COALESCE');
    expect(pkg).not.toContain('=>`SELECT ${alias}.* FROM (${source})');
    // The WITH scope point read and the owner lock still live here.
    expect(pkg.match(/WITH scope AS\(SELECT \?::text company_id/g)).toHaveLength(1);
    expect(pkg).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
  });

  it('leaves locks, native reads, r3-r29 families and cross-owner consumers untouched', async () => {
    const names = [
      'package-obligations.ts', 'asset-settlement-refs.ts', 'calendar-maintenance-facts.ts',
      'projection-lookup.ts', 'attachment-descriptor-read.ts', 'purchase-sources.ts',
    ];
    const texts = await Promise.all(names.map(src));
    expect(texts[0]).toContain('export const factRow=');
    expect(texts[0]).toContain('export const scopedSourceCte=');
    expect(texts[1]).toContain('export const settlementRefSource');
    expect(texts[1]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
    expect(texts[2]).toContain('export const scheduledSourceSql');
    expect(texts[3]).toContain('export const projectionHead');
    expect(texts[3]).toContain('export const existsGuardPrefix');
    expect(texts[4]).toContain('attachmentDescriptorReadSql');
    expect(texts[5]).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
    for (const text of texts) {
      expect(text).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
    }
  });

  it('retirement:ts-statements stays strictly below the r30 BEFORE=19 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(18);
    expect(total).toBeLessThan(19);
  });
});
