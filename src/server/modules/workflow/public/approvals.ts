/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { canOverrideStaffStep, staffOverrideGuards, currentApproval, taskRoute, openApproval, initializeLegacyApproval, stepBlock, approvalView, prepareDecision, closeApprovals, rerouteApproval } from "@/server/modules/workflow/adapters/approvals";
export type { ApprovalKind, ApprovalPhase, ApprovalInstance, ApprovalStep, ApprovalDecision, ApprovalView } from "@/server/modules/workflow/adapters/approvals";

export {participantSql} from "../adapters/approval-participant-projection";
export {expenseApprovalReadBatch} from "../adapters/approvals";
