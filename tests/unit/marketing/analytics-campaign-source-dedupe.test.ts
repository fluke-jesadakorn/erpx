import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

const enforced =
  'SELECT campaign.*,NULL::text branch_id,NULL::text department_id FROM marketing_campaigns campaign';
const legacy =
  'SELECT *,NULL::text branch_id,NULL::text department_id FROM marketing_campaigns';

describe('marketing analytics campaign projection dedupe', () => {
  it('emits byte-identical enforced and legacy campaign projections', async () => {
    const { marketingAnalyticsCampaignSql } = await import(
      '../../../packages/server-marketing/src/adapters/analytics-campaign-source-sql.ts'
    );
    expect(marketingAnalyticsCampaignSql(true)).toBe(enforced);
    expect(marketingAnalyticsCampaignSql(false)).toBe(legacy);
  });

  it('keeps one query token in the helper and no inline literal in the consumer', async () => {
    const helper = await readFile(adapter('analytics-campaign-source-sql.ts'), 'utf8');
    const source = await readFile(adapter('analytics-sources.ts'), 'utf8');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
    expect(source).not.toContain(enforced);
    expect(source).not.toContain(legacy);
    expect(source).toContain('campaign:marketingAnalyticsCampaignSql(true)');
    expect(source).toContain('campaign:marketingAnalyticsCampaignSql(false)');
  });

  it('preserves the outer source selection and binds', async () => {
    const source = await readFile(adapter('analytics-sources.ts'), 'utf8');
    expect(source).toContain('(${definitions[kind]}) marketing_report');
    expect(source).toContain('sql:legacy[kind],args:[] as unknown[]');
  });
});
