/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { agentSourceAllowed, validateAssistantGovernance, assistantGovernanceGuards, assistantGovernanceContext, recordAssistantInvocation } from "@erpx/server-agent/adapters/agent-governance";
/** Workspace and evidence-record reads are owner-native and fail closed. */
export {
  nativeAgentWorkspaceFromActor as agentWorkspace,
  nativeAgentEvidenceRecordFromActor as agentEvidenceRecord,
} from './native-agent-workspace';
export type { NativeAgentWorkspace, NativeAgentRecord } from './native-agent-workspace';
export type { AssistantGovernance } from "@erpx/server-agent/adapters/agent-governance";
