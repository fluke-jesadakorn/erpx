import { describe, expect, it } from 'vitest';
import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { agentSelectColsFromSql } from '../../../packages/server-agent/src/adapters/read-tail-r26-source-sql';
import { agentTurnEvidenceSql } from '../../../packages/server-agent/src/adapters/turn-evidence-projection-source';

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

/** Byte oracles captured from the pre-dedupe (BEFORE=47) helper templates, both
 * as the exact rendered string and its SHA-256. The four head literals folded
 * into one, so only the head count drops; every rendering below stays identical. */
const oracle: Record<string, { sql: string; sha: string }> = {
  catalog1: {
    sql: 'SELECT v.*,a.name agent_name FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id WHERE a.company_id=? ORDER BY v.created_at DESC',
    sha: '8609f1aed9323177ec91f1ce6754c4d2a0d15887534f782b772cb0d839211c47',
  },
  catalog2: {
    sql: 'SELECT x.*,v.version_number,a.name agent_name FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog a ON a.id=v.agent_id WHERE x.company_id=? ORDER BY x.updated_at DESC',
    sha: 'de3fabadb42f569c7f62031a7bc3bc925a46ffda8f85d95421ff8c37a726e6e0',
  },
  settingsStar: {
    sql: 'SELECT * FROM ai_company_settings WHERE company_id=?',
    sha: 'ae5050fdfbec978c4337786e073dc7618f07adef39bdaff681d28ee38bb85349',
  },
  settingsVersion: {
    sql: 'SELECT version FROM ai_company_settings WHERE company_id=?',
    sha: '62b83ed86a5e509059657ebb01bf430864bb378dab41fd1d111ecb48aaad83d9',
  },
  settingsMode: {
    sql: 'SELECT mode FROM ai_company_settings WHERE company_id=?',
    sha: '6a3a06ef718b9dd97c6374998a2ca1f21dbbbc39f7ad608ba18da5efc16a8784',
  },
  evidenceAssistant: {
    sql: 'SELECT t.id,t.question,t.paths_json,t.sources_json,t.answer_json,t.english_answer_json,t.state,t.created_at,e.governance_snapshot_json FROM ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id',
    sha: 'cb1e3959b6e2f960ecbb476abfdc72056127cecacc6b9ebfb11b802aadfd5846',
  },
  evidenceIntake: {
    sql: 'SELECT t.user_id,t.company_id,t.assignment_id,t.scope_key,t.question,t.paths_json,t.state,t.sources_json,t.answer_json,t.private_session_id,e.governance_snapshot_json FROM ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id',
    sha: 'dee3165716e1cda54166ab66fbce6cb4cb34803e98bfd2389a7042942392855b',
  },
  byIdStar: {
    sql: 'SELECT * FROM ai_workspace_turns WHERE id=?',
    sha: '2eb6e2d5a0b6b352ee6a6d42a000c8ef01e226b3565e581ee9da4c0a887e422c',
  },
  byIdState: {
    sql: 'SELECT state,answer_json FROM ai_workspace_turns WHERE id=?',
    sha: 'bdcb87ec2fcd1a25640973f3fecde7506221ac6b8201d6dcab974b6cdcaad5b3',
  },
};

const catalogTail = (from: string, companyAlias: string, suffix: string) =>
  `${from} JOIN agent_catalog a ON a.id=v.agent_id WHERE ${companyAlias}.company_id=? ORDER BY ${suffix}`;
const settingsTail = 'ai_company_settings WHERE company_id=?';
const evidenceTail = 'ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id';
const byIdTail = 'ai_workspace_turns WHERE id=?';

/** All four delegated renderings, keyed to the pre-dedupe oracles. */
const renderings: Array<[keyof typeof oracle, string]> = [
  ['catalog1', agentSelectColsFromSql('v.*,a.name agent_name', catalogTail('agent_versions v', 'a', 'v.created_at DESC'))],
  ['catalog2', agentSelectColsFromSql('x.*,v.version_number,a.name agent_name', catalogTail('agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id', 'x', 'x.updated_at DESC'))],
  ['settingsStar', agentSelectColsFromSql('*', settingsTail)],
  ['settingsVersion', agentSelectColsFromSql('version', settingsTail)],
  ['settingsMode', agentSelectColsFromSql('mode', settingsTail)],
  ['evidenceAssistant', agentSelectColsFromSql('t.id,t.question,t.paths_json,t.sources_json,t.answer_json,t.english_answer_json,t.state,t.created_at,e.governance_snapshot_json', evidenceTail)],
  ['evidenceIntake', agentSelectColsFromSql('t.user_id,t.company_id,t.assignment_id,t.scope_key,t.question,t.paths_json,t.state,t.sources_json,t.answer_json,t.private_session_id,e.governance_snapshot_json', evidenceTail)],
  ['byIdStar', agentSelectColsFromSql('*', byIdTail)],
  ['byIdState', agentSelectColsFromSql('state,answer_json', byIdTail)],
];

describe('Agent read-tail r26 dedupe (BEFORE=47)', () => {
  it('owns exactly one SELECT-column head renderer emitting `SELECT ${cols} FROM ${tail}`', async () => {
    expect(agentSelectColsFromSql('*', 'ai_company_settings WHERE company_id=?')).toBe('SELECT * FROM ai_company_settings WHERE company_id=?');
    expect(agentSelectColsFromSql('a,b', 't WHERE id=?')).toBe('SELECT a,b FROM t WHERE id=?');
    const helper = await src('adapters/read-tail-r26-source-sql.ts');
    expect(helper).toContain('export const agentSelectColsFromSql = (cols: string, tail: string) =>');
    expect(helper).toContain('`SELECT ${cols} FROM ${tail}`;');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
  });

  it('keeps all nine pre-dedupe renderings byte-for-byte (string, Buffer and SHA-256 oracles)', () => {
    for (const [key, actual] of renderings) {
      const { sql, sha: expectedSha } = oracle[key];
      expect(actual).toBe(sql);
      expect(Buffer.from(actual, 'utf8').equals(Buffer.from(sql, 'utf8'))).toBe(true);
      expect(sha(actual)).toBe(expectedSha);
    }
  });

  it('the exported turn-evidence helper still renders both witnesses byte-identically', () => {
    expect(agentTurnEvidenceSql('t.id,t.question,t.paths_json,t.sources_json,t.answer_json,t.english_answer_json,t.state,t.created_at,e.governance_snapshot_json')).toBe(oracle.evidenceAssistant.sql);
    expect(agentTurnEvidenceSql('t.user_id,t.company_id,t.assignment_id,t.scope_key,t.question,t.paths_json,t.state,t.sources_json,t.answer_json,t.private_session_id,e.governance_snapshot_json')).toBe(oracle.evidenceIntake.sql);
    expect(sha(agentTurnEvidenceSql('*'))).toBe(sha('SELECT * FROM ' + evidenceTail));
  });

  it('all four helpers delegate to the one head and author no SELECT literal', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).not.toContain('agentCatalogListSql');

    const rt = await src('adapters/ai-runtime.ts');
    expect(rt).toContain("const aiCompanySettingsSql = (cols: string) =>\n  agentSelectColsFromSql(cols, 'ai_company_settings WHERE company_id=?');");
    expect(rt).not.toMatch(/const aiCompanySettingsSql[\s\S]{0,80}`SELECT /);
    expect(rt).toContain(".prepare(aiCompanySettingsSql('*'))\n    .bind(companyId)");
    expect(rt).toContain("`COALESCE((${aiCompanySettingsSql('version')}),0)=?`");
    expect(rt).toContain('" AND COALESCE((" + aiCompanySettingsSql(\'mode\') + "),\'deployment\')=\'deployment\'"');

    const ev = await src('adapters/turn-evidence-projection-source.ts');
    expect(ev).toContain("agentSelectColsFromSql(cols, 'ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id');");
    expect(ev).not.toMatch(/SELECT /);

    const wa = await src('adapters/workspace-assistant.ts');
    expect(wa).toContain("const agentWorkspaceTurnByIdSql = (cols:string) => agentSelectColsFromSql(cols, 'ai_workspace_turns WHERE id=?');");
    expect(wa).not.toMatch(/SELECT /);
    expect(wa).toContain('agentWorkspaceTurnByIdSql("*")');
    expect(wa).toContain('agentWorkspaceTurnByIdSql("state,answer_json")');
  });

  it('keeps locks, native writes and cross-owner call sites byte-unchanged', async () => {
    const gov = await src('adapters/agent-governance.ts');
    expect(gov).toContain("const agentPoolMemberAccessSql = 'EXISTS(SELECT 1 FROM agent_pool_members m WHERE m.pool_id=x.pool_id AND m.user_id=? AND m.revoked_at IS NULL)';");
    expect(gov).toContain("const agentPoolMemberAccessByPoolSql = agentPoolMemberAccessSql.replace('x.pool_id', 'p.id');");
    expect(gov).toContain('db.prepare("SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE").bind(sessionId)');
    expect(gov).toContain('db.prepare("SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c").bind(assignmentId)');
    expect(gov).toContain('db.prepare("SELECT id FROM agent_pools WHERE id=? FOR SHARE").bind(poolId)');
    expect(gov).toContain('nativeOwnerWriteNoopSql');
    expect(gov).not.toContain('.bind(companyId)\n          .all<Row>()).results;},');

    const wa = await src('adapters/workspace-assistant.ts');
    expect(wa).toContain('db.prepare(agentWorkspaceTurnByIdSql("*")).bind(id).first<Row');
    expect(wa).toContain('db.prepare(agentWorkspaceTurnByIdSql("state,answer_json")).bind(id).first<Row>();');
    expect(wa).toContain('nativeOwnerWrite(');
    const intake = await src('adapters/operations-guided-intake-facts.ts');
    expect(intake).toContain('agentTurnEvidenceSql("t.user_id,t.company_id');
  });

  it('KPI: TS SELECT tokens in packages/server-agent/src drop from 47 to 42', async () => {
    expect(await agentSelectCount(new URL('../../../packages/server-agent/src/', import.meta.url))).toBe(38);
  });
});
