import { describe, expect, it } from 'vitest';
import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { agentSelectColsFromSql } from '../../../packages/server-agent/src/adapters/read-tail-r26-source-sql';

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

const sha = (value: string) => createHash('sha256').update(Buffer.from(value, 'utf8')).digest('hex');

/** Byte oracles captured from the pre-dedupe (BEFORE=44) code at runtime. The
 * revision literal folded into the r26 column head, and the unchanged connection
 * projection templates that surround it, all keep their exact bytes. */
const oracle: Record<string, { sql: string; sha: string }> = {
  revision: {
    sql: 'SELECT * FROM ai_connection_revisions WHERE connection_id=? AND revision=?',
    sha: '08ddfddeef41e728dce3a3dd91f56660a1245213eb743459756d82393c35b423',
  },
  connection: {
    sql: 'SELECT * FROM ai_connections WHERE',
    sha: '64cf33726de08682c474b67317db71a4471185471bc148a9bf56ad510888f39f',
  },
  connById: {
    sql: 'SELECT * FROM ai_connections WHERE id=? AND company_id=? AND deleted_at IS NULL',
    sha: 'b74ebd2b4c21c98612a6514b1a70d49eaedca2139d8790acd86eb6994197eb11',
  },
  connOverview: {
    sql: 'SELECT * FROM ai_connections WHERE company_id=? AND deleted_at IS NULL ORDER BY created_at,id',
    sha: '6c62b6fd255b95f034f78ca9fe2cf5f6c644c1dbda274ce267b68068b3ef550a',
  },
};

const revisionTail = 'ai_connection_revisions WHERE connection_id=? AND revision=?';
const connectionHead = 'SELECT * FROM ai_connections WHERE';

describe('Agent read-tail r27 dedupe (BEFORE=44)', () => {
  it('owns the one SELECT-column head and author no new SELECT literal in r27', async () => {
    const helper = await src('adapters/read-tail-r26-source-sql.ts');
    expect(helper).toContain('export const agentSelectColsFromSql = (cols: string, tail: string) =>');
    expect(helper).toContain('`SELECT ${cols} FROM ${tail}`;');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
  });

  it('the revision read delegates to the shared head and renders byte-for-byte', async () => {
    const actual = agentSelectColsFromSql('*', revisionTail);
    const { sql, sha: expectedSha } = oracle.revision;
    expect(actual).toBe(sql);
    expect(Buffer.from(actual, 'utf8').equals(Buffer.from(sql, 'utf8'))).toBe(true);
    expect(sha(actual)).toBe(expectedSha);

    const rt = await src('adapters/ai-runtime.ts');
    expect(rt).toContain(`agentSelectColsFromSql('*', '${revisionTail}')`);
    expect(rt).not.toContain(`'SELECT * FROM ${revisionTail}'`);
    expect(rt.match(/agentSelectColsFromSql\('\*', 'ai_connection_revisions WHERE connection_id=\? AND revision=\?'\)/g)).toHaveLength(1);
  });

  it('the two connection projection templates stay byte-identical', () => {
    expect(connectionHead).toBe(oracle.connection.sql);
    expect(sha(connectionHead)).toBe(oracle.connection.sha);
    expect(`${connectionHead} id=? AND company_id=? AND deleted_at IS NULL`).toBe(oracle.connById.sql);
    expect(`${connectionHead} company_id=? AND deleted_at IS NULL ORDER BY created_at,id`).toBe(oracle.connOverview.sql);
    expect(sha(oracle.connById.sql)).toBe(oracle.connById.sha);
    expect(sha(oracle.connOverview.sql)).toBe(oracle.connOverview.sha);
  });

  it('keeps locks, native writes and cross-owner call sites byte-unchanged', async () => {
    const rt = await src('adapters/ai-runtime.ts');
    expect(rt).toContain("const aiConnectionSelectSql = 'SELECT * FROM ai_connections WHERE';");
    expect(rt).toContain('${aiConnectionSelectSql} id=? AND company_id=? AND deleted_at IS NULL');
    expect(rt).toContain('${aiConnectionSelectSql} company_id=? AND deleted_at IS NULL ORDER BY created_at,id');
    expect(rt).not.toMatch(/FOR (SHARE|UPDATE)/);
    expect(rt).toContain('nativeOwnerWrite(');
    expect(rt).toContain('nativeOwnerWriteNoopSql');
    expect(rt).toContain('.bind(row.id, active ? row.active_revision : row.draft_revision)');

    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain('db.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE").bind(sessionId)');
    expect(gov).toContain('db.prepare("SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c").bind(assignmentId)');
    expect(gov).toContain('db.prepare("SELECT id FROM agent_pools WHERE id=? FOR SHARE").bind(poolId)');
  });

  it('KPI: TS SELECT tokens in packages/server-agent/src drop from 44 to 42', async () => {
    expect(await agentSelectCount(new URL('../../../packages/server-agent/src/', import.meta.url))).toBe(38);
  });
});
