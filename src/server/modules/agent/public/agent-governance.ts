/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { agentSourceAllowed, validateAssistantGovernance, assistantGovernanceGuards, agentWorkspace, createAgent, createAgentVersion, createAgentPool, changeAgentPool, changePrivateSession, createAgentAssignment, changeAgentRecord, assistantGovernanceContext, recordAssistantInvocation, agentEvidenceRecord } from "@/server/modules/agent/adapters/agent-governance";
export type { AssistantGovernance } from "@/server/modules/agent/adapters/agent-governance";
