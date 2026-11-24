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

/** Verbatim pre-dedupe catalog-joined company list readers (r20 BEFORE: 57 SELECTs). */
const versionsSql =
  'SELECT v.*,a.name agent_name FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE a.company_id=? ORDER BY v.created_at DESC';
const assignmentsSql =
  'SELECT x.*,v.version_number,a.name agent_name FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog a ON a.id=v.agent_id WHERE x.company_id=? ORDER BY x.updated_at DESC';

describe('Agent read-tail r20 catalog-joined company list dedupe', () => {
  it('authors the parameterized catalog-joined company list projection once', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(
      /const agentCatalogListSql = \(cols: string, from: string, companyAlias: string, suffix: string\) =>\s*`(SELECT [^`]*?)`;/,
    )?.[1];
    expect(def).toBe(
      'SELECT ${cols} FROM ${from} JOIN agent_catalog a ON a.id=v.agent_id WHERE ${companyAlias}.company_id=? ORDER BY ${suffix}',
    );
    expect(gov.match(/SELECT \$\{cols\} FROM \$\{from\} JOIN agent_catalog a/g)).toHaveLength(1);
    expect(gov.match(/agentCatalogListSql\(/g)).toHaveLength(2);
  });

  it('routes both catalog-joined readers through the shared projection with unchanged binds', async () => {
    const gov = await src('adapters/agent-governance.ts');
    const def = gov.match(
      /const agentCatalogListSql = \(cols: string, from: string, companyAlias: string, suffix: string\) =>\s*`(SELECT [^`]*?)`;/,
    )?.[1]!;
    const calls = [...gov.matchAll(/agentCatalogListSql\('([^']+)','([^']+)','([^']+)','([^']+)'\)/g)].map(
      (m) => [m[1], m[2], m[3], m[4]] as const,
    );
    expect(calls).toEqual([
      ['v.*,a.name agent_name', 'agent_versions v', 'a', 'v.created_at DESC'],
      [
        'x.*,v.version_number,a.name agent_name',
        'agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id',
        'x',
        'x.updated_at DESC',
      ],
    ]);
    expect(
      calls.map(([c, f, a, s]) =>
        def.replaceAll('${cols}', c).replaceAll('${from}', f).replaceAll('${companyAlias}', a).replaceAll('${suffix}', s),
      ),
    ).toEqual([versionsSql, assignmentsSql]);
    expect(gov).toContain("agentCatalogListSql('v.*,a.name agent_name','agent_versions v','a','v.created_at DESC'),\n          )\n          .bind(companyId)");
    expect(gov).toContain("agentCatalogListSql('x.*,v.version_number,a.name agent_name','agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id','x','x.updated_at DESC'),\n          )\n          .bind(companyId)");
    expect(gov).not.toContain(versionsSql);
    expect(gov).not.toContain(assignmentsSql);
  });

  it('records a strictly lower SELECT count than BEFORE=57', async () => {
    expect(await agentSelectCount(new URL('../../../packages/server-agent/src/', import.meta.url))).toBeLessThan(57);
  });

  it('leaves agentPoolMemberAccessSql, guards, FOR SHARE locks and prior families untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain(
      "const agentPoolMemberAccessSql = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)';",
    );
    expect(gov).toContain(`const privateSessionSelectSql = '${'SELECT s.id,s.user_id,s.assignment_id,s.conversation_key,s.generation,s.created_at,c.* FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=?'}'`);
    expect(gov).toContain('const agentActivePrivateSessionSql = (expiresAt: string) =>');
    expect(gov).toContain('const agentCompanyListSql = (table: string, suffix: string) =>');
    expect(gov).toContain('const agentPoolChildListSql = (table: string, alias: string, suffix: string) =>');
    for (const lock of [
      '"SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE"',
      '"SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c"',
      '"SELECT id FROM agent_pools WHERE id=? FOR SHARE"',
    ]) expect(gov).toContain(lock);
    expect(gov).toContain('guardDefaultPolicy(key:string,value:"agent-default",companyId:string,path:string,userId:string)');
    expect(gov).toContain('guardApprovedPolicy(key:string,value:"approved-agent",assignmentId:string');
    expect(gov).toContain('guardPrivateSession(key:string,value:"private-session",sessionId:string');
  });
});
