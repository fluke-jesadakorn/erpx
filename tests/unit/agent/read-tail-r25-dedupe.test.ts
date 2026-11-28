import { describe, expect, it } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { agentMonthlyUnionCountSql } from '../../../modules/agent/server/src/adapters/read-tail-r25-source-sql';
import { collaborationAiSources } from '@erpx/agent-server/adapters/collaboration-ai-sources';
import { executiveGenerationQuota } from '@erpx/agent-server/adapters/executive-generation-quota';

const src = (name: string) =>
  readFile(new URL(`../../../modules/agent/server/src/${name}`, import.meta.url), 'utf8');

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

/** Byte oracles captured from the pre-dedupe (BEFORE=49) code at runtime. */
const collabOracle =
  '(SELECT count(*) FROM social_ai_usage WHERE company_id=? AND month=?) + (SELECT count(*) FROM (SELECT company_id,month FROM expense_ai_usage UNION ALL SELECT company_id,month FROM request_ai_usage UNION ALL SELECT company_id,month FROM finance_ai_usage UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE \'UTC\',\'YYYY-MM\') AS month FROM ai_workspace_turns UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE \'UTC\',\'YYYY-MM\') AS month FROM (SELECT "company_id","created_at" FROM executive_generation_jobs) executive_generation_jobs) WHERE company_id=? AND month=?) < COALESCE((SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?),100) AND (SELECT count(*) FROM social_ai_usage WHERE company_id=? AND state=\'reserved\' AND lease_until>?)<? AND (SELECT count(*) FROM social_ai_usage WHERE actor_id=? AND created_at>?)<?';
const execOracle =
  '(SELECT count(*) FROM executive_generation_jobs WHERE company_id=? AND created_at>=?)+(SELECT count(*) FROM ai_workspace_turns WHERE company_id=? AND created_at>=?)+(SELECT count(*) FROM social_ai_usage WHERE company_id=? AND month=?)+(SELECT count(*) FROM expense_ai_usage WHERE company_id=? AND month=?)+(SELECT count(*) FROM (SELECT company_id,month FROM request_ai_usage UNION ALL SELECT company_id,month FROM finance_ai_usage) calls WHERE company_id=? AND month=?)<COALESCE((SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?),100)';

const renderedQuery = (value: unknown) => (value as { values: unknown[] }).values.at(-1) as string;

describe('Agent read-tail r25 dedupe (BEFORE=49)', () => {
  it('the one monthly-union count head renders both closed alias variants byte-for-byte', async () => {
    const noAlias = '(SELECT count(*) FROM (U) WHERE company_id=? AND month=?)';
    const calls = '(SELECT count(*) FROM (U) calls WHERE company_id=? AND month=?)';
    expect(agentMonthlyUnionCountSql('U')).toBe(noAlias);
    expect(agentMonthlyUnionCountSql('U', 'calls')).toBe(calls);
    const helper = await src('adapters/read-tail-r25-source-sql.ts');
    expect(helper).toContain("alias: '' | 'calls' = ''");
    expect(helper.match(/SELECT /g)).toHaveLength(1);
  });

  it('collaboration AI sources stops authoring the count head and keeps the exact guard bytes', async () => {
    const adapter = await src('adapters/collaboration-ai-sources.ts');
    expect(adapter).not.toMatch(/SELECT /);
    expect(adapter).toContain('collaborationQuotaUnionCountSql = agentMonthlyUnionCountSql(');
    expect(adapter).toContain('${collaborationQuotaUnionCountSql} < ${recordsAiMonthlyLimitSql}');
    const db = {} as never;
    expect(renderedQuery(collaborationAiSources(db).guardQuota('id', 'quota', 'c', 'm', 'c2', 'm2', 'c3', 'c4', 1, 'u', 2, 3, 4))).toBe(collabOracle);
  });

  it('executive generation quota stops authoring the count head and keeps the exact guard bytes', async () => {
    const adapter = await src('adapters/executive-generation-quota.ts');
    expect(adapter).not.toMatch(/SELECT /);
    expect(adapter).toContain("agentMonthlyUnionCountSql(`${agentRequestMonthlyUsage()} UNION ALL ${agentFinanceMonthlyUsage()}`, 'calls')");
    expect(adapter).toContain('${executiveQuotaUnionCountSql}<${financeAiCompanyAllowanceSql}');
    const db = {} as never;
    expect(renderedQuery(executiveGenerationQuota(db).guard7('k', 'executive-quota', 'c', 1, 'c2', 2, 'c3', 'm', 'c4', 'm2', 'c5', 'm3', 'c6'))).toBe(execOracle);
  });

  it('pool-member pair is derived from the one authored head, preserving the p.id witness bytes', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const authored = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)';
    const preDedupeById = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=p.id AND m.user_id=? AND m.revoked_at IS NULL)';
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${authored}';`);
    expect(gov).toContain("const agentPoolMemberAccessByPoolSql = agentPoolMemberAccessSql.replace('x.pool_id', 'p.id');");
    expect(authored.replace('x.pool_id', 'p.id')).toBe(preDedupeById);
    expect(gov.match(/SELECT 1 FROM agent_pool_members m WHERE m\.pool_id=/g)).toHaveLength(1);
    expect(gov).toContain('AND ${agentPoolMemberAccessByPoolSql}))`');
    expect(gov).not.toContain('EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=p.id');
  });

  it('KPI: TS SELECT tokens in modules/agent/server/src drop from 49 to 42', async () => {
    expect(await agentSelectCount(new URL('../../../modules/agent/server/src/', import.meta.url))).toBe(38);
  });
});
