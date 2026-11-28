import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';
import { crmMarketingCandidateSources } from '../../../modules/marketing/server/src/adapters/crm-purchase-sources';

const adapter = (name: string) =>
  new URL(`../../../modules/marketing/server/src/adapters/${name}`, import.meta.url);

const expected = {
  asset: "SELECT asset.id,asset.company_id,NULL::text branch_id,NULL::text department_id,(to_jsonb(asset)-'status'-'reviewed_by'-'version'-'updated_at') witness FROM marketing_assets asset",
  candidate: `SELECT candidate.id,candidate.company_id,candidate.branch_id,candidate.department_id,(to_jsonb(candidate)-'status'-'crm_lead_id'-'reviewed_by'-'version'-'updated_at') witness FROM (${crmMarketingCandidateSources}) candidate`,
  campaign: "SELECT campaign.id,campaign.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(campaign) witness FROM marketing_campaigns campaign",
  'campaign-cost': "SELECT cost.id,cost.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(cost) witness FROM marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id",
} as const;

describe('marketing purchase witness-row family', () => {
  it('keeps one fixed projection template and no inline row literals in consumers', async () => {
    const helper = await readFile(adapter('marketing-purchase-rows-sql.ts'), 'utf8');
    const purchases = await readFile(adapter('purchases.ts'), 'utf8');
    // The witness head now renders through marketingSelectHeadSql; neither the
    // renderer body nor its comment may smuggle a token into the owner KPI.
    expect(helper.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(helper).toContain('marketingSelectHeadSql(head, ` FROM ${spec.source}`)');
    expect(purchases).not.toContain('`SELECT id,company_id,branch_id,department_id,witness::text witness FROM (');
    expect(purchases).toContain("marketingSelectHeadSql('id,company_id,branch_id,department_id,witness::text witness'");
    for (const sql of Object.values(expected)) expect(purchases).not.toContain(sql);
    expect(purchases).toContain("marketingPurchaseRowsSql('asset')");
    expect(purchases).toContain("marketingPurchaseRowsSql('candidate')");
    expect(purchases).toContain("marketingPurchaseRowsSql('campaign')");
    expect(purchases).toContain("marketingPurchaseRowsSql('campaign-cost')");
    expect(purchases).not.toContain('crmMarketingCandidateSources,');
  });

  it('emits byte-identical output for every admitted variant', async () => {
    const { marketingPurchaseRowsSql } = await import(
      '../../../modules/marketing/server/src/adapters/marketing-purchase-rows-sql.ts'
    );
    for (const [variant, sql] of Object.entries(expected)) {
      expect(marketingPurchaseRowsSql(variant as keyof typeof expected)).toBe(sql);
    }
  });

  it('keeps downstream string interpolation of the exported rows unchanged', async () => {
    const record = await readFile(adapter('record-purchases.ts'), 'utf8');
    expect(record).toContain('${marketingPurchaseAssetRows}');
    expect(record).toContain('${marketingPurchaseCandidateRows}');
    const purchases = await readFile(adapter('purchases.ts'), 'utf8');
    expect(purchases).toContain("marketingSourceRowGuardSql('('+marketingPurchaseCandidateRows+') native_candidate','native_candidate.id='+alias+'.id AND native_candidate.company_id='+alias+'.company_id')");
  });
});
