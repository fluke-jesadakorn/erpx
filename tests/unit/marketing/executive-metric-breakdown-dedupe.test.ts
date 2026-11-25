import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

const purchased = '(SELECT 1 AS id)';
const label = 'LBL';
const scope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?)';
const candidates =
  'SELECT LBL label,count(*)::text value FROM ((SELECT 1 AS id)) c WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) GROUP BY 1 ORDER BY 1';
const engagements =
  'SELECT LBL label,count(*)::text value FROM marketing_engagement_events e JOIN ((SELECT 1 AS id)) c ON c.id=e.candidate_id WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1';

describe('marketing executive metric breakdown dedupe', () => {
  it('emits byte-identical candidate and engagement breakdown projections', async () => {
    const { marketingMetricBreakdownSql } = await import(
      '../../../packages/server-marketing/src/adapters/executive-metric-breakdown-sql.ts'
    );
    expect(marketingMetricBreakdownSql('candidates', purchased, label, scope)).toBe(candidates);
    expect(marketingMetricBreakdownSql('engagements', purchased, label, scope)).toBe(engagements);
  });

  it('keeps one query token in the shared renderer and no inline breakdown literal in the consumer', async () => {
    const helper = await readFile(adapter('executive-metric-breakdown-sql.ts'), 'utf8');
    const source = await readFile(adapter('executive-metric-projections.ts'), 'utf8');
    expect(helper.match(/SELECT /g)).toBeNull();
    expect(helper).toContain('marketingSelectHeadSql(');
    expect(source).not.toContain('label,count(*)::text value FROM (');
    expect(source).not.toContain('label,count(*)::text value FROM marketing_engagement_events');
    expect(source).toContain("marketingMetricBreakdownSql('candidates', purchased.sql, label, scopedAlias('c'))");
    expect(source).toContain("marketingMetricBreakdownSql('engagements', purchased.sql, label, scopedAlias('c'))");
  });

  it('preserves the outer binds and routes labels through the shared helper', async () => {
    const source = await readFile(adapter('executive-metric-projections.ts'), 'utf8');
    expect(source).toContain('values = [...purchased.args,...scopeArgs];');
    expect(source).toContain('values = [...purchased.args,...scopeArgs, Date.parse(f.from), Date.parse(f.to) + 86400000];');
    expect(source).not.toContain('const branchLabel = `COALESCE((SELECT');
    expect(source).not.toContain('const departmentLabel = `COALESCE((SELECT');
    expect(source).toContain("const branchLabel = marketingExecutiveLabelSql('branch');");
    expect(source).toContain("const departmentLabel = marketingExecutiveLabelSql('department');");
  });

  it('emits byte-identical branch and department label projections from one helper', async () => {
    const { marketingExecutiveLabelSql } = await import(
      '../../../packages/server-marketing/src/adapters/executive-label-sql.ts'
    );
    const { commercialOwnerProjectionSources } = await import(
      '@erpx/server-organization/public/commercial-owner-projection-sources'
    );
    const nodes = commercialOwnerProjectionSources.organization_nodes;
    expect(marketingExecutiveLabelSql('branch')).toBe(
      `COALESCE((SELECT n.display_name FROM (${nodes}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`
    );
    expect(marketingExecutiveLabelSql('department')).toBe(
      `COALESCE((SELECT n.display_name FROM (${nodes}) n WHERE n.id=c.department_id AND n.company_id=c.company_id),'Unassigned')`
    );
    const helper = await readFile(adapter('executive-label-sql.ts'), 'utf8');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
  });
});
