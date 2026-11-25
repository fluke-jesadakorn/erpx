import { agentSelectColsFromSql } from './read-tail-r26-source-sql';
import 'server-only';
/** One Agent-owned workspace-turn and invocation-evidence projection shared by the
 * assistant conversation-turns and guided-intake previous-turn reads. Only the
 * column list varies; each caller keeps its own predicates and binds, and the
 * native evidence owner remains the authority. */
export const agentTurnEvidenceSql = (cols: string) =>
  agentSelectColsFromSql(cols, 'ai_workspace_turns t LEFT JOIN ai_invocation_evidence e ON e.turn_id=t.id');
