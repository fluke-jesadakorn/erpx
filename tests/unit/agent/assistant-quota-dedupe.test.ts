import { describe, expect, it } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';

const src = (name: string) =>
  readFile(new URL(`../../../packages/server-agent/src/${name}`, import.meta.url), 'utf8');

async function agentSelectCount(dir: URL): Promise<number> {
  let total = 0;
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const child = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
    if (entry.isDirectory()) total += await agentSelectCount(child);
    else if (entry.name.endsWith('.ts')) {
      total += ((await readFile(child, 'utf8')).match(/SELECT/g) ?? []).length;
    }
  }
  return total;
}

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

  /** Verbatim pre-dedupe private-session projection (workspaceSessions ↔ sessionRecord). */
  const privateSessionSelect = 'SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=?';

  it('r11: authors the byte-identical private-session projection once', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain(`const privateSessionSelectSql = '${privateSessionSelect}';`);
    expect(gov.match(/SELECT s\.id,s\.user_id,s\.assignment_id,s\.conversation_key/g)).toHaveLength(1);
    expect(gov.match(/\$\{privateSessionSelectSql\}/g)).toHaveLength(2);
  });

  it('r11: both consumers render the exact pre-dedupe statement text with unchanged binds', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(/const privateSessionSelectSql = '([^']*)';/)?.[1];
    expect(def).toBe(privateSessionSelect);
    const templates = [...gov.matchAll(/`([\s\S]*?)`/g)]
      .map((m) => m[1])
      .filter((t) => t.includes('${privateSessionSelectSql}'));
    expect(templates).toHaveLength(2);
    expect(templates.map((t) => t.replace('${privateSessionSelectSql}', def!))).toEqual([
      `${privateSessionSelect} AND (s.user_id=? OR ?) ORDER BY c.updated_at DESC LIMIT 100`,
      `${privateSessionSelect} AND s.id=? AND (s.user_id=? OR ?)`,
    ]);
    expect(gov).toContain('.bind(companyId,userId,value).all<Row>()).results;},');
    expect(gov).toContain('.bind(companyId,id,userId,value).first<Row>();},async batch');
  });

  it('r11: the translation guard and r9/r10 constants stay local and untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const assistant = await src('adapters/workspace-assistant.ts');
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${poolMemberAccess}';`);
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota"');
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
  });

  /** Verbatim pre-dedupe gateway monthly-request count projection. */
  const gatewayRequestCount = '(SELECT count(*)::bigint FROM ai_gateway_requests r';

  it('r12: authors the byte-identical gateway monthly-request count once', async () => {
    const gw = await src('adapters/ai-gateway.ts');
    expect(gw).toContain(`const aiGatewayRequestCountSql = '${gatewayRequestCount}';`);
    expect(gw.match(/SELECT count\(\*\)::bigint FROM ai_gateway_requests r/g)).toHaveLength(1);
    expect(gw.match(/\$\{aiGatewayRequestCountSql\}/g)).toHaveLength(2);
  });

  it('r12: both gateway consumers render the exact pre-dedupe text with unchanged binds', async () => {
    const gw = await src('adapters/ai-gateway.ts');
    const def = gw.match(/const aiGatewayRequestCountSql = '([^']*)';/)?.[1];
    expect(def).toBe(gatewayRequestCount);
    const rendered = (body: string) => body.replace(/\$\{aiGatewayRequestCountSql\}/g, def!);
    const lines = gw.split('\n').map((line) => line.trim());
    expect(rendered(lines.find((line) => line.endsWith('monthly_requests,'))!)).toBe(
      `${gatewayRequestCount} WHERE r.created_at>=? AND r.created_at<?) monthly_requests,`,
    );
    expect(rendered(lines.find((line) => line.includes('${aiGatewayRequestCountSql} LEFT JOIN'))!)).toBe(
      `${gatewayRequestCount} LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id`,
    );
    expect(gw).toContain('.bind(start,end,start,end,start,end)');
  });

  /** Verbatim pre-dedupe connection projection (connection ↔ aiProviderOverview). */
  const runtimeConnectionSelect = 'SELECT * FROM ai_connections WHERE';

  it('r12: authors the byte-identical connection projection once', async () => {
    const rt = await src('adapters/ai-runtime.ts');
    expect(rt).toContain(`const aiConnectionSelectSql = '${runtimeConnectionSelect}';`);
    expect(rt.match(/SELECT \* FROM ai_connections WHERE/g)).toHaveLength(1);
    expect(rt.match(/\$\{aiConnectionSelectSql\}/g)).toHaveLength(2);
  });

  it('r12: both connection consumers render the exact pre-dedupe text with unchanged binds', async () => {
    const rt = await src('adapters/ai-runtime.ts');
    const def = rt.match(/const aiConnectionSelectSql = '([^']*)';/)?.[1];
    expect(def).toBe(runtimeConnectionSelect);
    const templates = [...rt.matchAll(/`([\s\S]*?)`/g)]
      .map((m) => m[1])
      .filter((t) => t.includes('${aiConnectionSelectSql}'));
    expect(templates).toHaveLength(2);
    expect(templates.map((t) => t.replace('${aiConnectionSelectSql}', def!))).toEqual([
      'SELECT * FROM ai_connections WHERE id=? AND company_id=? AND deleted_at IS NULL',
      'SELECT * FROM ai_connections WHERE company_id=? AND deleted_at IS NULL ORDER BY created_at,id',
    ]);
    expect(rt).toContain('.bind(id, companyId)');
    expect(rt).toContain('.bind(actor.companyId)');
  });

  it('r12: the r7-r11 constants, translation guard and source locks stay untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const helper = await src('adapters/assistant-quota-sql.ts');
    const assistant = await src('adapters/workspace-assistant.ts');
    const locks = await src('adapters/social-ai-source-locks.ts');
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${poolMemberAccess}';`);
    expect(gov).toContain(`const privateSessionSelectSql = '${privateSessionSelect}';`);
    expect(helper).toContain(quotaExpression);
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota"');
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
    expect(locks.match(/SELECT/g)).toHaveLength(4);
    expect(locks).toContain("'settings',(SELECT to_jsonb(s) FROM ai_company_settings s WHERE s.company_id=?)");
  });

  /** Verbatim pre-dedupe parameterized company-settings projection. */
  const companySettingsTemplate = 'SELECT ${cols} FROM ai_company_settings WHERE company_id=?';
  const companySettingsRendered = (col: string) =>
    `SELECT ${col} FROM ai_company_settings WHERE company_id=?`;

  it('r13: authors the parameterized company-settings projection once', async () => {
    const rt = await src('adapters/ai-runtime.ts');
    const def = rt.match(/const aiCompanySettingsSql = \(cols: string\) =>\s*`(SELECT [^`]*?)`;/)?.[1];
    expect(def).toBe(companySettingsTemplate);
    expect(rt.match(/SELECT \$\{cols\} FROM ai_company_settings WHERE company_id=\?/g)).toHaveLength(1);
    expect(rt.match(/aiCompanySettingsSql\(/g)).toHaveLength(3);
  });

  it('r13: all three consumers render the exact pre-dedupe SQL with unchanged binds', async () => {
    const rt = await src('adapters/ai-runtime.ts');
    const def = rt.match(/const aiCompanySettingsSql = \(cols: string\) =>\s*`(SELECT [^`]*?)`;/)?.[1]!;
    const args = [...rt.matchAll(/aiCompanySettingsSql\('([^']+)'\)/g)].map((m) => m[1]);
    expect(args).toEqual(['*', 'version', 'mode']);
    expect(args.map((col) => def.replace('${cols}', col))).toEqual([
      companySettingsRendered('*'),
      companySettingsRendered('version'),
      companySettingsRendered('mode'),
    ]);
    // currentSelection keeps its single companyId bind.
    expect(rt).toContain(".prepare(aiCompanySettingsSql('*'))\n    .bind(companyId)");
    // The two guard reads keep the same COALESCE wrappers and bind order.
    expect(rt).toContain("`COALESCE((${aiCompanySettingsSql('version')}),0)=?`");
    expect(rt).toContain('" AND COALESCE((" + aiCompanySettingsSql(\'mode\') + "),\'deployment\')=\'deployment\'"');
    expect(rt).toContain('snapshot.companyId,\n        snapshot.settingsVersion,');
    expect(rt).toContain(': [snapshot.companyId]),');
  });

  it('r13: the r7-r12 constants, translation guard and source locks stay untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const helper = await src('adapters/assistant-quota-sql.ts');
    const assistant = await src('adapters/workspace-assistant.ts');
    const locks = await src('adapters/social-ai-source-locks.ts');
    const rt = await src('adapters/ai-runtime.ts');
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${poolMemberAccess}';`);
    expect(gov).toContain(`const privateSessionSelectSql = '${privateSessionSelect}';`);
    expect(helper).toContain(quotaExpression);
    expect(rt).toContain(`const aiConnectionSelectSql = '${runtimeConnectionSelect}';`);
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota"');
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
    expect(locks.match(/SELECT/g)).toHaveLength(4);
    expect(locks).toContain("'settings',(SELECT to_jsonb(s) FROM ai_company_settings s WHERE s.company_id=?)");
  });

  /** Verbatim pre-dedupe parameterized active-private-session witness (r15 -> r16 BEFORE: 61 SELECTs).
   * activePrivateSession used the equivalent reversed inner join, normalized here to sessions-first. */
  const activeSessionTemplate =
    "SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>${expiresAt}";
  const activeSessionBoundBefore =
    "SELECT 1 FROM ai_private_session_controls c JOIN ai_private_sessions s ON s.id=c.session_id WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>?";

  it('r16: authors the parameterized active-private-session witness once', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(/const agentActivePrivateSessionSql = \(expiresAt: string\) =>\s*`(SELECT [^`]*?)`;/)?.[1];
    expect(def).toBe(activeSessionTemplate);
    expect(gov.match(/SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c\.session_id=s\.id/g)).toHaveLength(1);
    expect(gov.match(/agentActivePrivateSessionSql\(/g)).toHaveLength(2);
  });

  it('r16: both consumers render the exact witness with unchanged binds', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(/const agentActivePrivateSessionSql = \(expiresAt: string\) =>\s*`(SELECT [^`]*?)`;/)?.[1]!;
    const render = (exp: string) => def.replace('${expiresAt}', exp);
    expect(render('?').replace(
      'ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id',
      'ai_private_session_controls c JOIN ai_private_sessions s ON s.id=c.session_id',
    )).toBe(activeSessionBoundBefore);
    expect(render('floor(extract(epoch FROM clock_timestamp())*1000)::bigint')).toBe(
      "SELECT 1 FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.id=? AND s.company_id=? AND s.user_id=? AND s.assignment_id=? AND c.status='active' AND c.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint",
    );
    expect(gov).toContain("db.prepare(agentActivePrivateSessionSql('?'))");
    expect(gov).toContain('.bind(sessionId,companyId,userId,assignmentId,now).first()');
    expect(gov).toContain("`EXISTS(${agentActivePrivateSessionSql('floor(extract(epoch FROM clock_timestamp())*1000)::bigint')})`,[sessionId,companyId,userId,assignmentId]");
  });

  it('r16: the translation guard, locks and r7-r15 families stay untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const assistant = await src('adapters/workspace-assistant.ts');
    const locks = await src('adapters/social-ai-source-locks.ts');
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${poolMemberAccess}';`);
    expect(gov).toContain(`const privateSessionSelectSql = '${privateSessionSelect}';`);
    for (const lock of [
      '"SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE"',
      '"SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c"',
      '"SELECT id FROM agent_pools WHERE id=? FOR SHARE"',
    ]) expect(gov).toContain(lock);
    expect(locks.match(/SELECT/g)).toHaveLength(4);
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota"');
    expect(assistant).toContain('"SELECT state,answer_json FROM ai_workspace_turns WHERE id=?"');
  });

  /** Verbatim pre-dedupe parameterized company-scoped list projection
   * (workspaceAgents ↔ workspacePools ↔ workspaceInvocations). */
  const companyListTemplate = 'SELECT * FROM ${table} WHERE company_id=? ${suffix}';

  it('r17: authors the parameterized company-scoped list projection once', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(/const agentCompanyListSql = \(table: string, suffix: string\) =>\s*`(SELECT [^`]*?)`;/)?.[1];
    expect(def).toBe(companyListTemplate);
    expect(gov.match(/SELECT \* FROM \$\{table\} WHERE company_id=\? \$\{suffix\}/g)).toHaveLength(1);
    expect(gov.match(/agentCompanyListSql\(/g)).toHaveLength(3);
  });

  it('r17: all three consumers render the exact pre-dedupe SQL with unchanged binds', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(/const agentCompanyListSql = \(table: string, suffix: string\) =>\s*`(SELECT [^`]*?)`;/)?.[1]!;
    const calls = [...gov.matchAll(/agentCompanyListSql\('([^']+)','([^']+)'\)/g)].map((m) => [m[1], m[2]]);
    expect(calls).toEqual([
      ['agent_catalog', 'ORDER BY updated_at DESC'],
      ['agent_pools', 'ORDER BY name'],
      ['ai_invocation_evidence', 'ORDER BY created_at DESC LIMIT 50'],
    ]);
    expect(calls.map(([t, s]) => def.replace('${table}', t).replace('${suffix}', s))).toEqual([
      'SELECT * FROM agent_catalog WHERE company_id=? ORDER BY updated_at DESC',
      'SELECT * FROM agent_pools WHERE company_id=? ORDER BY name',
      'SELECT * FROM ai_invocation_evidence WHERE company_id=? ORDER BY created_at DESC LIMIT 50',
    ]);
    expect(gov).toContain("agentCompanyListSql('agent_catalog','ORDER BY updated_at DESC'),\n          )\n          .bind(companyId)");
  });

  it('r17: the translation guard, locks and r7-r16 families stay untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const assistant = await src('adapters/workspace-assistant.ts');
    const locks = await src('adapters/social-ai-source-locks.ts');
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${poolMemberAccess}';`);
    expect(gov).toContain(`const privateSessionSelectSql = '${privateSessionSelect}';`);
    expect(gov).toContain('const agentActivePrivateSessionSql = (expiresAt: string) =>');
    for (const lock of [
      '"SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE"',
      '"SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c"',
      '"SELECT id FROM agent_pools WHERE id=? FOR SHARE"',
    ]) expect(gov).toContain(lock);
    expect(locks.match(/SELECT/g)).toHaveLength(4);
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota"');
    expect(assistant).toContain('"SELECT state,answer_json FROM ai_workspace_turns WHERE id=?"');
  });

  /** Verbatim pre-dedupe pool-child list projections (workspaceMembers / workspacePoolHistory). */
  const memberListSql =
    'SELECT m.* FROM agent_pool_members m JOIN agent_pools p ON p.id=m.pool_id WHERE p.company_id=? ORDER BY m.pool_id,m.user_id';
  const poolHistoryListSql =
    'SELECT r.* FROM agent_pool_revisions r JOIN agent_pools p ON p.id=r.pool_id WHERE p.company_id=? ORDER BY r.created_at DESC';

  it('r18: authors the parameterized pool-child list projection once', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(
      /const agentPoolChildListSql = \(table: string, alias: string, suffix: string\) =>\s*`(SELECT [^`]*?)`;/,
    )?.[1];
    expect(def).toBe(
      'SELECT ${alias}.* FROM ${table} ${alias} JOIN agent_pools p ON p.id=${alias}.pool_id WHERE p.company_id=? ORDER BY ${suffix}',
    );
    expect(gov.match(/SELECT \$\{alias\}\.\* FROM \$\{table\}/g)).toHaveLength(1);
  });

  it('r18: both consumers render the exact pre-dedupe SQL with unchanged binds', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(
      /const agentPoolChildListSql = \(table: string, alias: string, suffix: string\) =>\s*`(SELECT [^`]*?)`;/,
    )?.[1]!;
    const calls = [
      ...gov.matchAll(/agentPoolChildListSql\('([^']+)','([^']+)','([^']+)'\)/g),
    ].map((m) => [m[1], m[2], m[3]]);
    expect(calls).toEqual([
      ['agent_pool_members', 'm', 'm.pool_id,m.user_id'],
      ['agent_pool_revisions', 'r', 'r.created_at DESC'],
    ]);
    expect(
      calls.map(([t, a, s]) =>
        def.replaceAll('${alias}', a).replaceAll('${table}', t).replaceAll('${suffix}', s),
      ),
    ).toEqual([memberListSql, poolHistoryListSql]);
    expect(gov).toContain(
      "db.prepare(agentPoolChildListSql('agent_pool_members','m','m.pool_id,m.user_id')).bind(companyId)",
    );
    expect(gov).toContain(
      "db.prepare(agentPoolChildListSql('agent_pool_revisions','r','r.created_at DESC')).bind(companyId)",
    );
  });

  it('r18: records a strictly lower SELECT count than BEFORE=58', async () => {
    expect(await agentSelectCount(new URL('../../../packages/server-agent/src/', import.meta.url))).toBeLessThan(58);
  });

  it('r18: the translation guard, locks and r7-r17 families stay untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const assistant = await src('adapters/workspace-assistant.ts');
    const locks = await src('adapters/social-ai-source-locks.ts');
    expect(gov).toContain(`const agentPoolMemberAccessSql = '${poolMemberAccess}';`);
    expect(gov).toContain(`const privateSessionSelectSql = '${privateSessionSelect}';`);
    expect(gov).toContain('const agentActivePrivateSessionSql = (expiresAt: string) =>');
    expect(gov).toContain('const agentCompanyListSql = (table: string, suffix: string) =>');
    expect(await src('adapters/native-owner-write.ts')).toContain('nativeOwnerWriteNoopSql = "SELECT 1"');
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string){return assertion(db,id,"translation-quota"');
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
    expect(assistant).toContain('"SELECT state,answer_json FROM ai_workspace_turns WHERE id=?"');
    for (const lock of [
      '"SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE"',
      '"SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c"',
      '"SELECT id FROM agent_pools WHERE id=? FOR SHARE"',
    ]) expect(gov).toContain(lock);
    expect(locks.match(/SELECT/g)).toHaveLength(4);
    expect(locks).toContain("'settings',(SELECT to_jsonb(s) FROM ai_company_settings s WHERE s.company_id=?)");
  });
});
