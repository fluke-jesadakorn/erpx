import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  readFile(new URL(`../../../packages/server-agent/src/${name}`, import.meta.url), 'utf8');

/** Verbatim pre-dedupe statements (r14 -> r15 BEFORE: 62 SELECTs). */
const conversationTurnsSql =
  "SELECT t.id,t.question,t.paths_json,t.sources_json,t.answer_json,t.english_answer_json,t.state,t.created_at,e.governance_snapshot_json FROM ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id WHERE t.company_id=? AND t.user_id=? AND t.assignment_id=? AND t.scope_key=? AND t.conversation_key=? AND t.private_session_id=? AND t.state='ready' ORDER BY t.created_at DESC LIMIT 20";
const guidedPreviousSql =
  "SELECT t.user_id,t.company_id,t.assignment_id,t.scope_key,t.question,t.paths_json,t.state,t.sources_json,t.answer_json,t.private_session_id,e.governance_snapshot_json FROM ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id WHERE t.id=?";

describe('Agent read-tail r15 turn-evidence projection family dedupe', () => {
  it('authors the workspace-turn/invocation-evidence projection once', async () => {
    const helper = await src('adapters/turn-evidence-projection-source.ts');
    expect(helper).toContain('export const agentTurnEvidenceSql = (cols: string) =>');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
  });

  it('routes both residual reads through the shared projection with their own columns, predicates and binds', async () => {
    const assistant = await src('adapters/workspace-assistant.ts');
    const intake = await src('adapters/operations-guided-intake-facts.ts');
    expect(assistant).toContain('agentTurnEvidenceSql("t.id,t.question');
    expect(intake).toContain('agentTurnEvidenceSql("t.user_id,t.company_id');
    expect(assistant.match(/agentTurnEvidenceSql\(/g)).toHaveLength(1);
    expect(intake.match(/agentTurnEvidenceSql\(/g)).toHaveLength(1);
    expect(assistant).toContain("WHERE t.company_id=? AND t.user_id=? AND t.assignment_id=? AND t.scope_key=? AND t.conversation_key=? AND t.private_session_id=? AND t.state='ready' ORDER BY t.created_at DESC LIMIT 20");
    expect(intake).toContain('WHERE t.id=?');
  });

  it('renders the exact pre-dedupe statement text for both callers', async () => {
    const { agentTurnEvidenceSql } = await import('../../../packages/server-agent/src/adapters/turn-evidence-projection-source');
    expect(
      agentTurnEvidenceSql('t.id,t.question,t.paths_json,t.sources_json,t.answer_json,t.english_answer_json,t.state,t.created_at,e.governance_snapshot_json') +
        " WHERE t.company_id=? AND t.user_id=? AND t.assignment_id=? AND t.scope_key=? AND t.conversation_key=? AND t.private_session_id=? AND t.state='ready' ORDER BY t.created_at DESC LIMIT 20",
    ).toBe(conversationTurnsSql);
    expect(
      agentTurnEvidenceSql('t.user_id,t.company_id,t.assignment_id,t.scope_key,t.question,t.paths_json,t.state,t.sources_json,t.answer_json,t.private_session_id,e.governance_snapshot_json') +
        ' WHERE t.id=?',
    ).toBe(guidedPreviousSql);
  });

  it('leaves the translation guard and the lock statements untouched', async () => {
    const assistant = await src('adapters/workspace-assistant.ts');
    expect(assistant).toContain('guardTranslationQuota(id:string,companyId:string,value:number,month:string)');
    expect(assistant).toContain(') calls WHERE company_id=? AND created_at>=?');
    expect(assistant).toContain('"SELECT state,answer_json FROM ai_workspace_turns WHERE id=?"');
    const governance = await src('adapters/agent-governance.ts');
    expect(governance).toContain('"SELECT session_id FROM ai_private_session_controls WHERE session_id=? FOR SHARE"');
    expect(governance).toContain('"SELECT x.id FROM agent_assignments x JOIN agent_versions v ON v.id=x.agent_version_id JOIN agent_catalog c ON c.id=v.agent_id WHERE x.id=? FOR SHARE OF x,v,c"');
    expect(governance).toContain('"SELECT id FROM agent_pools WHERE id=? FOR SHARE"');
  });
});
