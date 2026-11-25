import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingProjectionSourceSql } from '../../../packages/server-marketing/src/adapters/owner-projection-source';
import { marketingOwnerPointReadSql } from '../../../packages/server-marketing/src/adapters/owner-point-read-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const srcRoot = resolve('packages/server-marketing/src');

// Byte oracles captured from the eight original emitters before dedupe.
const projection = {
  campaigns: 'SELECT "id","company_id","code","name","created_by","created_at","version" FROM marketing_campaigns',
  costs: 'SELECT "id","company_id","campaign_id","journal_line_id","evidence","reviewed_by","created_at" FROM marketing_campaign_costs',
  candidates: 'SELECT "id","company_id","branch_id","department_id","form_id","form_version","contact_name","contact_detail","consent_evidence","attribution_json","brief","status","crm_lead_id","created_by","reviewed_by","version","created_at","updated_at" FROM marketing_lead_candidates',
  touches: 'SELECT "id","company_id","lead_id","campaign_id","source","evidence","source_kind","source_id","occurred_at","created_by" FROM marketing_touches',
} as const;
const assetFact =
  'SELECT company_id,NULL::text branch_id,NULL::text department_id,version,kind FROM marketing_assets WHERE id=? AND company_id=?';
const campaignFact = 'SELECT company_id,version FROM marketing_campaigns WHERE id=? AND company_id=?';
const witness = (projection: string, table: string) =>
  `SELECT ${projection} FROM ${table} source WHERE source.id=? AND source.company_id=?`;
const exists = 'SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?';
const record = (rows: string) => `SELECT kind FROM (${rows}) actual WHERE id=? AND company_id=?`;

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

describe('marketing projection-source and point-read head dedupe (r40)', () => {
  it('freezes the four closed owner projections from the one column-list/table head', async () => {
    expect(
      marketingProjectionSourceSql(
        ['id', 'company_id', 'code', 'name', 'created_by', 'created_at', 'version'],
        'marketing_campaigns',
      ),
    ).toBe(projection.campaigns);
    expect(
      marketingProjectionSourceSql(
        ['id', 'company_id', 'campaign_id', 'journal_line_id', 'evidence', 'reviewed_by', 'created_at'],
        'marketing_campaign_costs',
      ),
    ).toBe(projection.costs);
    expect(
      marketingProjectionSourceSql(
        ['id','company_id','branch_id','department_id','form_id','form_version','contact_name','contact_detail','consent_evidence','attribution_json','brief','status','crm_lead_id','created_by','reviewed_by','version','created_at','updated_at'],
        'marketing_lead_candidates',
      ),
    ).toBe(projection.candidates);
    expect(
      marketingProjectionSourceSql(
        ['id','company_id','lead_id','campaign_id','source','evidence','source_kind','source_id','occurred_at','created_by'],
        'marketing_touches',
      ),
    ).toBe(projection.touches);

    const { marketingCampaignProjection } = await import(
      '../../../packages/server-marketing/src/adapters/hospitality-owner-projection-sources'
    );
    const { marketingCampaignCostProjection } = await import(
      '../../../packages/server-marketing/src/adapters/last-audit-owner-projection-sources'
    );
    const { commercialOwnerProjectionSources } = await import(
      '../../../packages/server-marketing/src/adapters/commercial-owner-projection-sources'
    );
    expect(marketingCampaignProjection).toBe(projection.campaigns);
    expect(marketingCampaignCostProjection).toBe(projection.costs);
    expect(commercialOwnerProjectionSources).toEqual({
      marketing_campaign_costs: projection.costs,
      marketing_campaigns: projection.campaigns,
      marketing_lead_candidates: projection.candidates,
      marketing_touches: projection.touches,
    });
  });

  it('keeps one SELECT token in the projection head and none in the three wrappers', async () => {
    expect((await src('owner-projection-source.ts')).match(/SELECT /g)).toHaveLength(1);
    for (const name of [
      'commercial-owner-projection-sources.ts',
      'hospitality-owner-projection-sources.ts',
      'last-audit-owner-projection-sources.ts',
    ]) {
      expect(await src(name)).not.toMatch(/SELECT /);
      expect(await src(name)).toContain('marketingProjectionSourceSql(');
    }
  });

  it('freezes the four id+company point reads from the one qualify/source head', async () => {
    expect(marketingOwnerPointReadSql('company_id,NULL::text branch_id,NULL::text department_id,version,kind', 'marketing_assets')).toBe(assetFact);
    expect(marketingOwnerPointReadSql('company_id,version', 'marketing_campaigns')).toBe(campaignFact);
    expect(
      marketingOwnerPointReadSql(
        "(to_jsonb(source)-'version'-'updated_at')::text witness",
        'marketing_assets source',
        true,
      ),
    ).toBe(witness("(to_jsonb(source)-'version'-'updated_at')::text witness", 'marketing_assets'));
    expect(marketingOwnerPointReadSql('to_jsonb(source)::text image', 'marketing_campaigns source', true)).toBe(
      witness('to_jsonb(source)::text image', 'marketing_campaigns'),
    );
    expect(marketingOwnerPointReadSql('1', 'marketing_campaigns')).toBe(exists);
    expect(marketingOwnerPointReadSql('kind', '(source_rows) actual')).toBe(record('source_rows'));

    for (const sql of [assetFact, campaignFact, exists, record('source_rows'), witness('w', 'marketing_assets')]) {
      expect(sql.match(/\?/g)).toHaveLength(2);
    }
    expect(witness('w', 'marketing_assets')).toContain('source.id=? AND source.company_id=?');
    expect(assetFact).toContain('WHERE id=? AND company_id=?');
  });

  it('keeps one SELECT token in the point-read head and no inline copies in the four wrappers', async () => {
    expect((await src('owner-point-read-sql.ts')).match(/SELECT /g)).toHaveLength(1);
    for (const name of [
      'attachment-owner-fact-read-sql.ts',
      'attachment-source-witness.ts',
      'campaign-existence.ts',
    ]) {
      expect(await src(name)).not.toMatch(/SELECT /);
      expect(await src(name)).toContain('marketingOwnerPointReadSql(');
    }
    const purchases = await src('record-purchases.ts');
    expect(purchases).not.toContain('SELECT kind FROM (${rows}) actual');
    expect(purchases).toContain("marketingOwnerPointReadSql('kind',`(${rows}) actual`)");
  });

  it('preserves every consumer call and its two binds', async () => {
    const operational = await src('attachments-operational-facts.ts');
    const policy = await src('attachments-policy-facts.ts');
    expect(operational).toContain("attachmentOwnerFactReadSql('asset')).bind(facts.targetId,facts.companyId)");
    expect(policy).toContain("attachmentOwnerFactReadSql('campaign')).bind(facts.targetId,facts.companyId)");

    const attachmentPurchases = await src('attachment-purchases.ts');
    const attachmentRead = await src('attachment-read.ts');
    expect(attachmentPurchases).toContain(
      'attachmentSourceWitnessSql("(to_jsonb(source)-\'version\'-\'updated_at\')::text witness"',
    );
    expect(attachmentRead).toContain("attachmentSourceWitnessSql('to_jsonb(source)::text image'");
    for (const consumer of [attachmentPurchases, attachmentRead]) {
      expect(consumer).not.toContain('FROM ${table} source WHERE source.id=?');
    }

    const submission = await src('public-submission.ts');
    const campaigns = await src('campaigns.ts');
    expect(submission).toContain('db.prepare(marketingCampaignExistsSql).bind(id,companyId).first()');
    expect(campaigns).toContain('`EXISTS(${marketingCampaignExistsSql})`');
    expect(campaigns).toContain('[i.campaignId,a.companyId]');

    const purchases = await src('record-purchases.ts');
    expect(purchases).toContain("marketingOwnerPointReadSql('kind',`(${rows}) actual`)).bind(id,actor.companyId)");
  });

  it('leaves FOR SHARE, command_receipts settlement and native read ports untouched', async () => {
    expect(await src('form-asset-lock-sql.ts')).toContain(
      '`SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`',
    );
    expect(await src('package-obligations.ts')).toContain('LOCK TABLE marketing_assets,marketing_asset_events');
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('purchases.ts')).toContain('IN SHARE ROW EXCLUSIVE MODE');
    expect(await src('native-marketing-read.ts')).toContain('export');
    expect(await src('marketing.ts')).toContain('from "./native-marketing-read"');
  });

  it('retirement:ts-statements KPI is 23 (strictly below BEFORE=55)', async () => {
    // r42 merged the evidence-row tail (49 -> 48); r43 retires the source-row,
    // actor-scope and union-frame families (48 -> 38); r44 retires the EXISTS
    // and point-existence heads (38 -> 36); r45 folds the three read-tail heads
    // into one renderer (36 -> 34); r46 folds two byte-identical head pairs (34 -> 32);
    // r47 folds three plain heads (32 -> 29); r48 routes the two purchase witness
    // heads through marketingSelectHeadSql (29 -> 27); r49 folds the three
    // parenthesized subquery heads (27 -> 24); r50 folds the actor-scope
    // assignment guard head (24 -> 23).
    expect(await selectTokens()).toBe(22);
  });
});
