import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  readFile(new URL(`../../../modules/agent/server/src/${name}`, import.meta.url), 'utf8');

/** Verbatim pre-dedupe monthly-usage column projection (records ↔ finance). */
const monthlyUsageCols = "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')";

describe('Agent read-tail r8 monthly-usage source dedupe', () => {
  it('authors the byte-identical monthly-usage projection once', async () => {
    const helper = await src('adapters/agent-monthly-usage-source.ts');
    expect(helper).toContain(`export const agentMonthlyUsageColsSql = "${monthlyUsageCols}";`);
    expect(helper.match(/SELECT company_id,to_char\(/g)).toHaveLength(1);
  });

  it('routes both consumers through the shared core with unchanged rendered SQL', async () => {
    const records = await src('adapters/records-ai-quota.ts');
    const finance = await src('adapters/finance-ai-quota-sources.ts');
    expect(records).toContain("export const agentWorkspaceMonthlyUsageSql = `${agentMonthlyUsageColsSql} AS month FROM ai_workspace_turns`;");
    expect(finance).toContain('export const financeAiAgentUsageSql=`${agentMonthlyUsageColsSql} FROM ai_workspace_turns UNION ALL ${agentMonthlyUsageColsSql} FROM (${runtimeProjectionSources.executive_generation_jobs}) executive_generation_jobs`;');
    expect(records).not.toContain(monthlyUsageCols);
    expect(finance).not.toContain(monthlyUsageCols);
    expect(records.match(/agentMonthlyUsageColsSql/g)).toHaveLength(2);
    expect(finance.match(/agentMonthlyUsageColsSql/g)).toHaveLength(3);
  });

  it('renders the exact pre-dedupe statement text for every caller', async () => {
    const { agentMonthlyUsageColsSql } = await import('../../../modules/agent/server/src/adapters/agent-monthly-usage-source');
    expect(agentMonthlyUsageColsSql).toBe(monthlyUsageCols);
    expect(`${agentMonthlyUsageColsSql} AS month FROM ai_workspace_turns`).toBe(
      `${monthlyUsageCols} AS month FROM ai_workspace_turns`,
    );
    expect(`${agentMonthlyUsageColsSql} FROM ai_workspace_turns UNION ALL ${agentMonthlyUsageColsSql} FROM (jobs) executive_generation_jobs`).toBe(
      `${monthlyUsageCols} FROM ai_workspace_turns UNION ALL ${monthlyUsageCols} FROM (jobs) executive_generation_jobs`,
    );
  });

  it('leaves the distinct translation guard untouched', async () => {
    const assistant = await src('adapters/workspace-assistant.ts');
    // The translation guard delegates with the closed `calls` alias.
    expect(assistant).toContain("`${assistantQuotaSql('calls')}`");
  });
});
