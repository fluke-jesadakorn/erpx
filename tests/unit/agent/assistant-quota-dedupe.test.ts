import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  readFile(new URL(`../../../packages/server-agent/src/${name}`, import.meta.url), 'utf8');

/** Verbatim pre-dedupe expression (guided-intake ↔ workspace assistant). */
const quotaExpression = '(SELECT count(*) FROM (SELECT company_id,created_at FROM ai_workspace_turns UNION ALL ${agentExecutiveDatedUsage()} UNION ALL ${agentFinanceDatedUsage()}) ai_calls WHERE company_id=? AND created_at>=?)+(${agentSocialMonthlyCount()})+(${agentExpenseMonthlyCount()})+(${agentRequestMonthlyCount()})<${financeAiCompanyAllowanceSql}';

describe('Agent read-tail r7 fixed SQL dedupe', () => {
  it('authors the assistant quota expression once, byte-identically', async () => {
    const helper = await src('adapters/assistant-quota-sql.ts');
    expect(helper.match(/`([\s\S]*?)`/g)?.map((text) => text.slice(1, -1))).toContain(quotaExpression);
    expect(helper.match(/ai_calls WHERE company_id=\? AND created_at>=\?/g)).toHaveLength(1);
  });

  it('routes both assistant-quota guards through the shared expression with unchanged binds', async () => {
    const guided = await src('adapters/collaboration-guided-intake-quota.ts');
    const assistant = await src('adapters/workspace-assistant.ts');
    expect(guided).toContain('`${assistantQuotaSql()}`');
    expect(guided).not.toContain('ai_calls WHERE');
    expect(guided).toContain('[companyId, value2, companyId2, month, companyId3, month2, companyId4, month3, companyId5]');
    expect(assistant).toContain('guardQuota(id:string,companyId:string,value:number,value2:string){return assertion(db,id,"assistant-quota",`${assistantQuotaSql()}`');
    expect(assistant).toContain('[companyId,value,companyId,value2,companyId,value2,companyId,value2,companyId]');
    // The translation guard's distinct `calls` alias stays local and untouched.
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
    expect(assistant.match(/ai_calls WHERE/g)).toBeNull();
  });

  it('shares the canonical native-write no-op statement literal', async () => {
    const noop = await src('adapters/native-owner-write.ts');
    const governance = await src('adapters/agent-governance.ts');
    const runtime = await src('adapters/ai-runtime.ts');
    expect(noop.match(/"SELECT 1"/g)).toHaveLength(1);
    expect(governance).not.toContain('"SELECT 1"');
    expect(governance.match(/db\.prepare\(nativeOwnerWriteNoopSql\)/g)).toHaveLength(3);
    expect(runtime).not.toContain('"SELECT 1"');
    expect(runtime.match(/db\.prepare\(nativeOwnerWriteNoopSql\)/g)).toHaveLength(1);
  });

  /** Verbatim pre-dedupe records quota count (quotaAllowed ↔ guardQuota). */
  const recordsQuotaCount = '(SELECT count(*) FROM (${recordsRequestUsageSql} UNION ALL ${recordsFinanceUsageSql} UNION ALL ${agentWorkspaceMonthlyUsageSql} UNION ALL ${recordsExecutiveUsageSql}) calls WHERE company_id=? AND month=?)';

  it('r9: authors the byte-identical records quota count once', async () => {
    const helper = await src('adapters/collaboration-agent-sources.ts');
    expect(helper).toContain(`const agentRecordsQuotaCountSql = \`${recordsQuotaCount}\`;`);
    expect(helper.match(/SELECT count\(\*\) FROM \(\$\{recordsRequestUsageSql\}/g)).toHaveLength(1);
    expect(helper.match(/agentRecordsQuotaCountSql/g)).toHaveLength(3);
  });

  it('r9: both consumers render the exact pre-dedupe statement text', async () => {
    const helper = await src('adapters/collaboration-agent-sources.ts');
    const def = helper.match(/const agentRecordsQuotaCountSql = `([\s\S]*?)`;/)?.[1];
    expect(def).toBe(recordsQuotaCount);
    const templates = [...helper.matchAll(/`([\s\S]*?)`/g)].map((m) => m[1]).filter((t) => t.includes('${agentRecordsQuotaCountSql}'));
    expect(templates).toHaveLength(2);
    expect(templates.map((t) => t.replace('${agentRecordsQuotaCountSql}', def!))).toEqual([
      `SELECT CASE WHEN
      \${socialMonthlyUsageCountSql} +
      \${recordsExpenseUsageCountSql} +
      (SELECT count(*) FROM (\${recordsRequestUsageSql} UNION ALL \${recordsFinanceUsageSql} UNION ALL \${agentWorkspaceMonthlyUsageSql} UNION ALL \${recordsExecutiveUsageSql}) calls WHERE company_id=? AND month=?) < \${recordsAiMonthlyLimitSql}
      AND \${socialRequestRateBudgetSql} THEN 1 ELSE 0 END AS allowed`,
      `\${socialMonthlyUsageCountSql} + \${recordsExpenseUsageCountSql} + (SELECT count(*) FROM (\${recordsRequestUsageSql} UNION ALL \${recordsFinanceUsageSql} UNION ALL \${agentWorkspaceMonthlyUsageSql} UNION ALL \${recordsExecutiveUsageSql}) calls WHERE company_id=? AND month=?) < \${recordsAiMonthlyLimitSql} AND \${socialRequestRateBudgetSql}`,
    ]);
  });

  it('r9: the distinct translation guard stays local and untouched', async () => {
    const assistant = await src('adapters/workspace-assistant.ts');
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota"');
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
  });

  /** Verbatim pre-dedupe pool-member access (policyCandidates ↔ guardDefaultPolicy). */
  const poolMemberAccess = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)';

  it('r10: authors the byte-identical pool-member access EXISTS once', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${poolMemberAccess}';`);
    expect(gov.match(/SELECT 1 FROM agent_pool_members m WHERE m\.pool_id=x\.pool_id/g)).toHaveLength(1);
    expect(gov.match(/\$\{agentPoolMemberAccessSql\}/g)).toHaveLength(2);
  });

  it('r10: both consumers render the exact pre-dedupe EXISTS text', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const rendered = (body: string) => body.replace('${agentPoolMemberAccessSql}', poolMemberAccess);
    expect(rendered('AND (x.user_id=? OR ${agentPoolMemberAccessSql})')).toBe(`AND (x.user_id=? OR ${poolMemberAccess})`);
    expect(gov).toContain('policyCandidates(companyId:string,userId:string)');
    expect(gov).toContain('guardDefaultPolicy(key:string,value:"agent-default"');
  });
});
