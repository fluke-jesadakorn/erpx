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

describe('Agent read-tail r20 catalog-joined company list retired to native', () => {
  it('the TS repository no longer authors the catalog-joined list projection', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).not.toContain('agentCatalogListSql');
    expect(gov).not.toContain(versionsSql);
    expect(gov).not.toContain(assignmentsSql);
  });

  it('the native port owns the byte-equivalent catalog joins with Company binds', async () => {
    const port = await src('adapters/workspace_read_postgres.rs');
    expect(port).toContain('SELECT v.*,a.name agent_name FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE a.company_id=$1');
    expect(port).toContain('SELECT x.*,v.version_number,a.name agent_name FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog a ON a.id=v.agent_id WHERE x.company_id=$1');
    expect(port).toContain('t.created_at DESC');
    expect(port).toContain('t.updated_at DESC');
  });

  it('records a strictly lower SELECT count than BEFORE=57', async () => {
    expect(await agentSelectCount(new URL('../../../packages/server-agent/src/', import.meta.url))).toBeLessThan(57);
  });

  it('leaves the retained guard, lock and write-read families untouched', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain(
      "const agentPoolMemberAccessSql = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)';",
    );
    expect(gov).not.toContain('privateSessionSelectSql');
    expect(gov).not.toContain('agentCompanyListSql');
    expect(gov).not.toContain('agentPoolChildListSql');
    expect(gov).toContain('const agentActivePrivateSessionSql = (expiresAt: string) =>');
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
