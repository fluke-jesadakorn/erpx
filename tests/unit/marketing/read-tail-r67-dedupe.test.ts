import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import {
  crmMarketingCandidateSources,
  crmMarketingCampaignSources,
} from '../../../modules/marketing/server/src/adapters/crm-purchase-sources';

const src = (name: string) =>
  readFile(new URL(`../../../modules/marketing/server/src/adapters/${name}`, import.meta.url), 'utf8');

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
  await walk(resolve('modules/marketing/server/src'));
  return total;
}

describe('marketing read-tail r67 CRM projection cutover', () => {
  it('keeps the exported projections byte-identical for the server-crm consumer', () => {
    expect(crmMarketingCampaignSources).toBe('SELECT id,company_id FROM marketing_campaigns');
    expect(crmMarketingCandidateSources).toBe(`SELECT candidate.* FROM marketing_lead_candidates candidate
 JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version
 JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active'
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'
 WHERE length(trim(candidate.consent_evidence))>=3`);
  });

  it('deletes both local SELECT tokens and delegates to the shared head', async () => {
    const source = await src('crm-purchase-sources.ts');
    expect(source.match(/SELECT /g)).toBeNull();
    expect(source).toContain("import {marketingSelectHeadSql} from './select-head-sql';");
    expect(source).toContain("marketingSelectHeadSql(`candidate.* FROM marketing_lead_candidates candidate");
    expect(source).toContain("marketingSelectHeadSql('id,company_id',' FROM marketing_campaigns')");
    expect(source).toContain('export const crmMarketingCandidateSources=marketingSelectHeadSql(');
  });

  it('retirement:ts-statements is BEFORE=3 -> AFTER=1', async () => {
    expect(await selectTokens()).toBe(1);
  });
});
