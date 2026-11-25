import { describe, expect, it } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { assistantQuotaSql } from '../../../packages/server-agent/src/adapters/assistant-quota-sql';
import { recordsQuotaCountSql, recordsAiQuota } from '@erpx/server-agent/adapters/records-ai-quota';
import { agentTurnCountGuardSql, agentAssistantRateGuardSql, GuidedIntakeAgentFacts } from '@erpx/server-agent/adapters/operations-guided-intake-facts';
import { collaborationAgentSources } from '@erpx/server-agent/adapters/collaboration-agent-sources';
import { executiveGenerationQuota } from '@erpx/server-agent/adapters/executive-generation-quota';

const src = (name: string) =>
  readFile(new URL(`../../../packages/server-agent/src/${name}`, import.meta.url), 'utf8');

async function agentSelectCount(dir: URL): Promise<number> {
  let total = 0;
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const child = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
    if (entry.isDirectory()) total += await agentSelectCount(child);
    else if (entry.name.endsWith('.ts')) {
      total += ((await readFile(child, 'utf8')).match(/SELECT /g) ?? []).length;
    }
  }
  return total;
}

/** Byte oracles captured from the pre-dedupe (BEFORE=53) code at runtime. */
const assistantUnion =
  'SELECT company_id,created_at FROM ai_workspace_turns UNION ALL SELECT company_id,created_at FROM executive_generation_jobs UNION ALL SELECT company_id,created_at FROM finance_ai_usage';
const assistantTail =
  ' WHERE company_id=? AND created_at>=?)+(SELECT count(*) FROM social_ai_usage WHERE company_id=? AND month=?)+(SELECT count(*) FROM expense_ai_usage WHERE company_id=? AND month=?)+(SELECT count(*)::int count FROM request_ai_usage WHERE company_id=? AND month=?)<COALESCE((SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?),100)';
const recordsUnion =
  "SELECT company_id,month FROM request_ai_usage UNION ALL SELECT company_id,month FROM finance_ai_usage UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM ai_workspace_turns UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM (SELECT \"company_id\",\"created_at\" FROM executive_generation_jobs) executive_generation_jobs";

const oracle = {
  f1AiCalls: `(SELECT count(*) FROM (${assistantUnion}) ai_calls${assistantTail}`,
  f1Calls: `(SELECT count(*) FROM (${assistantUnion}) calls${assistantTail}`,
  f2LowerCalls: `(SELECT count(*) FROM (${recordsUnion}) calls WHERE company_id=? AND month=?)`,
  f2UpperNoAlias: `(SELECT COUNT(*) FROM (${recordsUnion}) WHERE company_id=? AND month=?)`,
  f3Ge: '(SELECT count(*) FROM ai_workspace_turns WHERE company_id=? AND created_at>=?)',
  f3Gt: '(SELECT count(*) FROM ai_workspace_turns WHERE company_id=? AND created_at>?)',
  f3Rate: '(SELECT count(*) FROM ai_workspace_turns WHERE company_id=? AND created_at>?)<20',
};

const descriptor = (value: unknown) => value as { values: unknown[] };
const renderedQuery = (value: unknown) => descriptor(value).values.at(-1) as string;

describe('Agent read-tail r24 dedupe (BEFORE=53)', () => {
  it('f1: the parameterized alias renders both pre-dedupe quota statements byte-for-byte', async () => {
    expect(assistantQuotaSql('ai_calls')).toBe(oracle.f1AiCalls);
    expect(assistantQuotaSql('calls')).toBe(oracle.f1Calls);
    const helper = await src('adapters/assistant-quota-sql.ts');
    expect(helper).toContain("alias: 'ai_calls' | 'calls'");
    expect(helper.match(/\$\{alias\} WHERE company_id=\? AND created_at>=\?/g)).toHaveLength(1);
    expect(helper.match(/SELECT /g)).toHaveLength(2);
  });

  it('f1: all three quota guards delegate with the closed alias and keep 9/9/9 binds in order', async () => {
    const assistant = await src('adapters/workspace-assistant.ts');
    const guided = await src('adapters/collaboration-guided-intake-quota.ts');
    expect(assistant).toContain("`${assistantQuotaSql('ai_calls')}`,[companyId,value,companyId,value2,companyId,value2,companyId,value2,companyId]");
    expect(assistant).toContain("`${assistantQuotaSql('calls')}`,[companyId,value,companyId,month,companyId,month,companyId,month,companyId]");
    expect(guided).toContain("`${assistantQuotaSql('ai_calls')}`, [companyId, value2, companyId2, month, companyId3, month2, companyId4, month3, companyId5]");
    expect(assistant.match(/assistantQuotaSql\('calls'\)/g)).toHaveLength(1);
    expect(assistant.match(/assistantQuotaSql\('ai_calls'\)/g)).toHaveLength(1);
    expect(guided.match(/assistantQuotaSql\('ai_calls'\)/g)).toHaveLength(1);
  });

  it('f2: one builder renders both pre-dedupe records quota counts byte-for-byte', async () => {
    expect(recordsQuotaCountSql('count', 'calls')).toBe(oracle.f2LowerCalls);
    expect(recordsQuotaCountSql('COUNT')).toBe(oracle.f2UpperNoAlias);
    const db = {} as never;
    expect(renderedQuery(recordsAiQuota(db).guard('k', 's', ['c', 'm'], 'co'))).toContain(oracle.f2UpperNoAlias);
    expect(renderedQuery(collaborationAgentSources(db).guardQuota('u', 'quota', 'c', 'm', 'c2', 'm2', 'c3', 'm3', 'c4', 'c5', 1, 'u2', 2, 3, 4))).toContain(oracle.f2LowerCalls);
    const helper = await src('adapters/collaboration-agent-sources.ts');
    expect(helper).toContain("const agentRecordsQuotaCountSql = recordsQuotaCountSql('count', 'calls');");
    expect(helper.match(/recordsQuotaCountSql\('count', 'calls'\)/g)).toHaveLength(1);
  });

  it('f3: the closed comparator union renders both pre-dedupe heads byte-for-byte', () => {
    expect(agentTurnCountGuardSql('>=')).toBe(oracle.f3Ge);
    expect(agentTurnCountGuardSql('>')).toBe(oracle.f3Gt);
    expect(agentAssistantRateGuardSql).toBe(oracle.f3Rate);
    const db = {} as never;
    expect(renderedQuery(executiveGenerationQuota(db).guard7('k', 'executive-quota', 'c', 1, 'c2', 2, 'c3', 'm', 'c4', 'm2', 'c5', 'm3', 'c6'))).toContain(oracle.f3Ge);
    expect(renderedQuery(GuidedIntakeAgentFacts(db).guard3('id', 'assistant-rate', 'c', 42))).toBe(oracle.f3Rate);
  });

  it('KPI: TS SELECT tokens in packages/server-agent/src drop from 53 to 42', async () => {
    expect(await agentSelectCount(new URL('../../../packages/server-agent/src/', import.meta.url))).toBe(38);
  });
});
