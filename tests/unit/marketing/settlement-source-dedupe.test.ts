import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

const original = {
  engagements: 'SELECT * FROM marketing_engagement_events',
  costs: 'SELECT * FROM marketing_campaign_costs',
  touches: 'SELECT * FROM marketing_touches',
  campaigns: 'SELECT * FROM marketing_campaigns',
} as const;
const candidate = 'SELECT * FROM marketing_lead_candidates';

describe('marketing settlement source dedupe', () => {
  it('keeps one canonical SELECT literal in the shared head and no inline copies in the source', async () => {
    const head = await readFile(adapter('read-tail-sql.ts'), 'utf8');
    const helper = await readFile(adapter('marketing-plain-source-sql.ts'), 'utf8');
    const source = await readFile(adapter('package-obligations.ts'), 'utf8');
    for (const sql of Object.values(original)) expect(source).not.toContain(`'${sql}'`);
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(head).toContain("marketingSelectHeadSql('*', ` FROM ${table}${tail}`)");
    expect(helper.match(/SELECT /g)).toBeNull();
    expect(helper).toContain('marketingReadTailSql(table)');
    expect(source).toContain('settlementSourceRows');
    expect(source).toContain('marketingPlainSourceSql(source)');
  });

  it('emits the original byte-identical settlement source statements', async () => {
    const { marketingPackageSettlementSources } = await import(
      '../../../packages/server-marketing/src/adapters/package-obligations.ts'
    );
    for (const [variant, sql] of Object.entries(original)) {
      expect(marketingPackageSettlementSources[variant as keyof typeof original]).toBe(sql);
    }
  });

  it('keeps the four reads for the cross-owner CRM and Finance consumers', async () => {
    const source = await readFile(adapter('package-obligations.ts'), 'utf8');
    expect(source).not.toContain('WITH scope AS(SELECT ?::text company_id)');
    expect(source).not.toContain("database.prepare(`WITH scope AS");
    expect(source).toContain('nativeMarketingReadAll<{kind:string;id:string;fact:unknown;pending:boolean}>(MARKETING_RD_PACKAGE_OBLIGATIONS');
    expect(source).toContain('assertMarketingPackageObligationSources(owner,database,input.companyId)');
    expect(source).toContain('LOCK TABLE marketing_assets,marketing_asset_events,marketing_lead_candidates,marketing_engagement_events,marketing_campaigns,marketing_campaign_costs,marketing_touches,marketing_public_receipts,marketing_public_limits,command_receipts IN SHARE MODE');
    for (const owner of ['crm', 'finance']) {
      const consumer = await readFile(
        new URL(`../../../packages/server-${owner}/src/adapters/marketing-package-obligations.ts`, import.meta.url),
        'utf8',
      );
      expect(consumer).toContain('marketingPackageSettlementSources as marketing');
      expect(consumer).toContain('marketing.');
    }
  });

  it('shares the one projection frame with the legacy candidate reader', async () => {
    const { marketingPlainSourceSql } = await import(
      '../../../packages/server-marketing/src/adapters/marketing-plain-source-sql.ts'
    );
    const { marketingCandidateSourceSql } = await import(
      '../../../packages/server-marketing/src/adapters/marketing-candidate-source.ts'
    );
    expect(marketingPlainSourceSql('candidate')).toBe(candidate);
    expect(marketingCandidateSourceSql).toBe(candidate);
    const source = await readFile(adapter('marketing-candidate-source.ts'), 'utf8');
    expect(source).not.toContain(`'${candidate}'`);
    expect(source).toContain("marketingPlainSourceSql('candidate')");
  });
});
