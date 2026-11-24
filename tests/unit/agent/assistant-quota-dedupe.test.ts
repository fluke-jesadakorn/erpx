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
});
