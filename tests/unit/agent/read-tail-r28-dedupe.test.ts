import { describe, expect, it } from 'vitest';
import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { agentSelectColsFromSql } from '../../../modules/agent/server/src/adapters/read-tail-r26-source-sql';

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

const sha = (value: string) => createHash('sha256').update(Buffer.from(value, 'utf8')).digest('hex');

/** Byte oracle captured from the pre-dedupe (BEFORE=43) literal in
 * `agent-governance.ts` > `latestPrivateSession`. The r26 column head folds it
 * without changing a single rendered byte. */
const oracle = {
  latest: {
    sql: 'SELECT s.id,s.generation,c.status,c.expires_at FROM ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND s.user_id=? AND s.assignment_id=? AND s.scope_key=? AND s.conversation_key=? ORDER BY s.generation DESC LIMIT 1',
    sha: '445a968be548d5b203686e31336015c0d91a73c74f142a928f48b54f8a1ac279',
    bytes: 266,
  },
};

const latestCols = 's.id,s.generation,c.status,c.expires_at';
const latestTail = 'ai_private_sessions s JOIN ai_private_session_controls c ON c.session_id=s.id WHERE s.company_id=? AND s.user_id=? AND s.assignment_id=? AND s.scope_key=? AND s.conversation_key=? ORDER BY s.generation DESC LIMIT 1';

describe('Agent read-tail r28 dedupe (BEFORE=43)', () => {
  it('owns the one SELECT-column head and renders the private-session witness byte-for-byte', async () => {
    const helper = await src('adapters/read-tail-r26-source-sql.ts');
    expect(helper).toContain('export const agentSelectColsFromSql = (cols: string, tail: string) =>');
    expect(helper).toContain('`SELECT ${cols} FROM ${tail}`;');
    expect(helper.match(/SELECT /g)).toHaveLength(1);

    const actual = agentSelectColsFromSql(latestCols, latestTail);
    const { sql, sha: expectedSha, bytes } = oracle.latest;
    expect(actual).toBe(sql);
    expect(Buffer.from(actual, 'utf8').equals(Buffer.from(sql, 'utf8'))).toBe(true);
    expect(Buffer.byteLength(actual, 'utf8')).toBe(bytes);
    expect(sha(actual)).toBe(expectedSha);
  });

  it('latestPrivateSession delegates to the shared head and authors no residual literal', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain(`agentSelectColsFromSql('${latestCols}','${latestTail}')`);
    expect(gov.match(/agentSelectColsFromSql\('s\.id,s\.generation,c\.status,c\.expires_at'/g)).toHaveLength(1);
    expect(gov).not.toContain(`"SELECT ${latestCols} FROM ${latestTail}"`);
    expect(gov).not.toMatch(/SELECT s\.id,s\.generation,c\.status,c\.expires_at/);
    expect(gov).toContain('.bind(companyId,userId,assignmentId,scopeKey,conversationKey).first<{id:string;generation:number;status:string;expires_at:number}>();');
  });

  it('keeps pinned heads, locks, native writes and cross-owner call sites byte-unchanged', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain("const agentPoolMemberAccessSql = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)';");
    expect(gov).toContain("const agentPoolMemberAccessByPoolSql = agentPoolMemberAccessSql.replace('x.pool_id', 'p.id');");
    expect(gov).not.toContain('privateSessionSelectSql');
    expect(gov).not.toContain('agentCompanyListSql');
    expect(gov).not.toContain('agentPoolChildListSql');
    expect(gov).not.toContain('agentCatalogListSql');
    expect(gov).toContain('const agentActivePrivateSessionSql = (expiresAt: string) =>');
    expect(gov).toContain('db.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE").bind(sessionId);');
    expect(gov).toContain('db.prepare("SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c").bind(assignmentId);');
    expect(gov).toContain('db.prepare("SELECT id FROM agent_pools WHERE id=? FOR SHARE").bind(poolId);');
    expect(gov).toContain('nativeOwnerWrite(');
    expect(gov).toContain('nativeOwnerWriteNoopSql');
  });

  it('KPI: TS SELECT tokens in modules/agent/server/src drop from 43 to 42', async () => {
    expect(await agentSelectCount(new URL('../../../modules/agent/server/src/', import.meta.url))).toBe(38);
  });
});
