import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { marketingCompanyListSql } from '../../../modules/marketing/server/src/adapters/marketing-company-list-sql';
import { marketingClockedReportFrameSql } from '../../../modules/marketing/server/src/adapters/marketing-clocked-report-frame';
import { marketingPurchaseUnionSql } from '../../../modules/marketing/server/src/adapters/marketing-purchase-union-sql';

const adapter = (name: string) =>
  new URL(`../../../modules/marketing/server/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const srcRoot = resolve('modules/marketing/server/src');

// Byte oracles captured from the original emitters before dedupe.
const assetList = 'SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC';
const pendingScope =
  " AND ${await marketingCandidateReadFilter('marketing_lead_candidates')} AND status='pending' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)";
const candidateList = `SELECT * FROM marketing_lead_candidates WHERE company_id=?${pendingScope} ORDER BY updated_at DESC`;

const clock = '(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)';
const reportFrame = `WITH marketing_report_clock AS MATERIALIZED${clock} SELECT marketing_report.* FROM (SOURCE) marketing_report CROSS JOIN marketing_report_clock WHERE `;
const metricFrame = `WITH marketing_metric_clock AS MATERIALIZED ${clock} SELECT marketing_metric_owned.* FROM (SOURCE) marketing_metric_owned CROSS JOIN marketing_metric_clock WHERE `;
const scopeFrame = `WITH marketing_clock AS MATERIALIZED${clock} SELECT 1 FROM (ROWS) marketing_source CROSS JOIN marketing_clock WHERE `;

const unionArms = [
  ["asset.id,'asset' kind,asset.company_id,asset.branch_id,asset.department_id", '(${marketingPurchaseAssetRows}) asset'],
  ["candidate.id,'candidate',candidate.company_id,candidate.branch_id,candidate.department_id", '(${marketingPurchaseCandidateRows}) candidate'],
  ["id,'campaign',company_id,NULL,NULL", 'marketing_campaigns'],
  ["cost.id,'campaign-cost',cost.company_id,NULL,NULL", 'marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id'],
] as const;
const purchaseRows =
  "SELECT asset.id,'asset' kind,asset.company_id,asset.branch_id,asset.department_id FROM (${marketingPurchaseAssetRows}) asset UNION ALL SELECT candidate.id,'candidate',candidate.company_id,candidate.branch_id,candidate.department_id FROM (${marketingPurchaseCandidateRows}) candidate UNION ALL SELECT id,'campaign',company_id,NULL,NULL FROM marketing_campaigns UNION ALL SELECT cost.id,'campaign-cost',cost.company_id,NULL,NULL FROM marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id";

async function selectTokens(): Promise<number> {
  let total = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts'))
        total += ((await readFile(path, 'utf8')).match(/SELECT /g) ?? []).length;
    }
  };
  await walk(srcRoot);
  return total;
}

describe('marketing company-list head dedupe (r41)', () => {
  it('emits byte-identical asset and pending-candidate lists from one head', () => {
    expect(marketingCompanyListSql('marketing_assets', '')).toBe(assetList);
    expect(marketingCompanyListSql('marketing_lead_candidates', pendingScope)).toBe(candidateList);
    expect(assetList.match(/\?/g)).toHaveLength(1);
    expect(candidateList.match(/\?/g)).toHaveLength(5);
  });

  it('keeps one token in the shared head and no inline list in the repository', async () => {
    expect((await src('read-tail-sql.ts')).match(/SELECT /g) ?? []).toHaveLength(0);
    expect((await src('marketing-company-list-sql.ts')).match(/SELECT /g)).toBeNull();
    const repo = await src('marketing.ts');
    expect(repo).not.toContain('"SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC"');
    expect(repo).not.toContain('`SELECT * FROM marketing_lead_candidates WHERE company_id=? AND ');
    expect(repo).not.toContain("marketingCompanyListSql('marketing_assets','')");
    expect(repo).toContain('nativeMarketingReadAll<Row>(MARKETING_RD_ASSET_LIST');
    expect(repo).toContain(
      "marketingCompanyListSql('marketing_lead_candidates',` AND ${await marketingCandidateReadFilter('marketing_lead_candidates')} AND status='pending' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)`)",
    );
  });
});

describe('marketing materialized clocked report frame dedupe (r41)', () => {
  it('emits byte-identical frames for all three gaps and projections', () => {
    expect(
      marketingClockedReportFrameSql('marketing_report_clock', '', 'marketing_report.*', 'SOURCE', 'marketing_report'),
    ).toBe(reportFrame);
    expect(
      marketingClockedReportFrameSql('marketing_metric_clock', ' ', 'marketing_metric_owned.*', 'SOURCE', 'marketing_metric_owned'),
    ).toBe(metricFrame);
    expect(marketingClockedReportFrameSql('marketing_clock', '', '1', 'ROWS', 'marketing_source')).toBe(scopeFrame);
  });

  it('keeps one token in the frame and no inline frame in the three consumers', async () => {
    expect((await src('marketing-clocked-report-frame.ts')).match(/SELECT /g) ?? []).toHaveLength(0);
    const analytics = await src('analytics-sources.ts');
    const metric = await src('metric-purchases.ts');
    const record = await src('record-purchases.ts');
    expect(analytics).not.toContain('WITH marketing_report_clock AS MATERIALIZED');
    expect(metric).not.toContain('WITH marketing_metric_clock AS MATERIALIZED');
    expect(record).not.toContain('WITH marketing_clock AS MATERIALIZED');
    expect(analytics).toContain(
      "marketingClockedReportFrameSql('marketing_report_clock','', 'marketing_report.*', definitions[kind], 'marketing_report')",
    );
    expect(metric).toContain(
      "marketingClockedReportFrameSql('marketing_metric_clock',' ', 'marketing_metric_owned.*', crmMarketingCandidateSources, 'marketing_metric_owned')",
    );
    expect(record).toContain(
      "EXISTS(${marketingClockedReportFrameSql('marketing_clock','', '1', rows, 'marketing_source')}marketing_source.id=${id}",
    );
  });

  it('preserves alias-qualified now_ms, PMS touch branch and the EXISTS wrapper', async () => {
    const analytics = await src('analytics-sources.ts');
    const metric = await src('metric-purchases.ts');
    const record = await src('record-purchases.ts');
    expect(analytics).toContain("'marketing_report_clock.now_ms'");
    expect(analytics).toContain("'pms.reservations','history','marketing_report_clock.now_ms'");
    expect(analytics).toContain("kind==='touch'?`AND (marketing_report.source_kind<>'pms'");
    expect(metric).toContain('"marketing_metric_clock.now_ms"');
    expect(record).toContain("'marketing_clock.now_ms'");
    expect(record).toContain('EXISTS(${marketingClockedReportFrameSql(');
    expect(record).toContain("EXISTS(${marketingModuleMemberSql('selected')}))`");
  });
});

describe('marketing purchase union arm dedupe (r41)', () => {
  it('emits the byte-identical four-arm union with exact UNION ALL spacing', () => {
    expect(marketingPurchaseUnionSql(unionArms)).toBe(purchaseRows);
    expect(purchaseRows.match(/ UNION ALL /g)).toHaveLength(3);
    expect(purchaseRows).toContain(
      "UNION ALL SELECT id,'campaign',company_id,NULL,NULL FROM marketing_campaigns UNION ALL",
    );
    expect(purchaseRows).toContain(
      'marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id',
    );
  });

  it('keeps one token in the helper and no inline arms in the consumer', async () => {
    expect((await src('marketing-purchase-union-sql.ts')).match(/SELECT /g) ?? []).toHaveLength(0);
    const record = await src('record-purchases.ts');
    expect(record).not.toContain("const rows=`SELECT asset.id,'asset' kind");
    expect(record).toContain('const rows=marketingPurchaseUnionSql([');
    expect(record).toContain('"id,\'campaign\',company_id,NULL,NULL",\'marketing_campaigns\'');
  });
});

describe('marketing r41 retirement invariants', () => {
  it('leaves the native guard, FOR SHARE, settlement and native read ports untouched', async () => {
    const repo = await src('marketing.ts');
    expect(repo).toContain('async assets(){return (await nativeMarketingReadAll<Row>(MARKETING_RD_ASSET_LIST');
    expect(await src('public-form.ts')).toContain('`${lock.sql} FOR SHARE`');
    expect(await src('public-submission.ts')).toContain('`${lock.sql} FOR SHARE`');
    expect(await src('purchases.ts')).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_COMMAND_RECEIPT_PRIOR');
    expect(await src('native-marketing-read.ts')).toContain('export');
  });

  it('retirement:ts-statements KPI is 1 (BEFORE=55; lane 40 retired 3)', async () => {
    expect(await selectTokens()).toBe(1);
  });
});
