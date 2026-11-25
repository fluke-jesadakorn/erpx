import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingAnalyticsRowsSql } from '../../../packages/server-marketing/src/adapters/analytics-null-scope-rows';
import { marketingAnalyticsCampaignSql } from '../../../packages/server-marketing/src/adapters/analytics-campaign-source-sql';
import { marketingAnalyticsCostRows } from '../../../packages/server-marketing/src/adapters/analytics-cost-rows';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const srcRoot = resolve('packages/server-marketing/src');

// Byte oracles captured from the three original readers before dedupe.
const enforced =
  'SELECT campaign.*,NULL::text branch_id,NULL::text department_id FROM marketing_campaigns campaign';
const legacy =
  'SELECT *,NULL::text branch_id,NULL::text department_id FROM marketing_campaigns';
const cost = (source: string) =>
  `SELECT cost.*,NULL::text branch_id,NULL::text department_id FROM ${source}`;

async function selectTokens(): Promise<number> {
  let total = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) total += ((await readFile(path, 'utf8')).match(/SELECT /g) ?? []).length;
    }
  };
  await walk(srcRoot);
  return total;
}

describe('marketing NULL-scope analytics row-frame dedupe (r39)', () => {
  it('renders the enforced, legacy and cost projections byte-identically from one head', () => {
    expect(marketingAnalyticsRowsSql('campaign.*', 'marketing_campaigns campaign')).toBe(enforced);
    expect(marketingAnalyticsRowsSql('*', 'marketing_campaigns')).toBe(legacy);
    expect(marketingAnalyticsRowsSql('cost.*', 'marketing_campaign_costs cost')).toBe(
      cost('marketing_campaign_costs cost'),
    );
  });

  it('keeps one statement token in the shared renderer and none in the three wrappers', async () => {
    const head = await src('analytics-null-scope-rows.ts');
    expect(head.match(/SELECT /g)).toBeNull();
    expect(head).toContain('marketingSelectHeadSql(');
    expect(head.match(/NULL::text branch_id,NULL::text department_id/g)).toHaveLength(1);
    expect(head.match(/FROM \$\{source\}/g)).toHaveLength(1);

    for (const name of ['analytics-campaign-source-sql.ts', 'analytics-cost-rows.ts']) {
      expect((await src(name)).match(/SELECT /g)).toBeNull();
    }
  });

  it('leaves only delegating wrappers with the original public signatures', () => {
    expect(marketingAnalyticsCampaignSql(true)).toBe(enforced);
    expect(marketingAnalyticsCampaignSql(false)).toBe(legacy);
    expect(marketingAnalyticsCostRows('marketing_campaign_costs cost')).toBe(
      cost('marketing_campaign_costs cost'),
    );
  });

  it('keeps the analytics-sources consumers, variants and binds untouched', async () => {
    const analytics = await src('analytics-sources.ts');
    expect(analytics).toContain("import {marketingAnalyticsCostRows} from './analytics-cost-rows'");
    expect(analytics).toContain("import {marketingAnalyticsCampaignSql} from './analytics-campaign-source-sql'");
    expect(analytics).toContain('campaign:marketingAnalyticsCampaignSql(true)');
    expect(analytics).toContain('campaign:marketingAnalyticsCampaignSql(false)');
    expect(analytics.match(/marketingAnalyticsCostRows\(/g)).toHaveLength(2);
    expect(analytics).toContain(
      "marketingAnalyticsCostRows('marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id')",
    );
    expect(analytics).toContain("marketingAnalyticsCostRows('marketing_campaign_costs cost')");
  });

  it('leaves locks, settlement receipts and native read ports untouched', async () => {
    expect(await src('form-asset-lock-sql.ts')).toContain(
      '`SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`',
    );
    const obligations = await src('package-obligations.ts');
    expect(obligations).toContain("command.operation='marketing.asset.activate'");
    expect(obligations).toContain('LOCK TABLE marketing_assets,marketing_asset_events');
    const purchases = await src('purchases.ts');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain('IN SHARE ROW EXCLUSIVE MODE');
    expect(await src('native-marketing-read.ts')).toContain('export');
    expect(await src('marketing.ts')).toContain('from "./native-marketing-read"');
  });

  it('retirement:ts-statements KPI is 13 (strictly below BEFORE=55; lane 40 retired 3)', async () => {
    // r42 merged the evidence-row tail (49 -> 48); r43 retires six source-row
    // guards, three actor-scope guards and one union frame (48 -> 38); r44
    // retires the EXISTS and point-existence heads (38 -> 36); r45 folds the
    // three read-tail heads into one renderer (36 -> 34); r46 folds two
    // byte-identical head pairs (34 -> 32); r47 folds three plain heads (32 -> 29);
    // r48 routes the two purchase witness heads through marketingSelectHeadSql (29 -> 27);
    // r49 folds the three parenthesized subquery heads (27 -> 24); r50 folds the
    // actor-scope assignment guard head (24 -> 23); lane 40 retired the
    // candidate-form-rows helper, campaign aggregate head (19 -> 17) and the
    // public-submission receipt literal (17 -> 16).
    expect(await selectTokens()).toBe(13);
  });
});
