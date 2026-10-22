/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { canOverrideStaffStep, staffOverrideGuards, currentApproval, taskRoute, openApproval, initializeLegacyApproval, stepBlock, approvalView, prepareDecision, closeApprovals, rerouteApproval } from "@erpx/server-workflow/adapters/approvals";
export type { ApprovalKind, ApprovalPhase, ApprovalInstance, ApprovalStep, ApprovalDecision, ApprovalView } from "@erpx/server-workflow/adapters/approvals";

export {participantSql} from "@erpx/server-workflow/adapters/approval-participant-projection";
export {expenseApprovalReadBatch} from "@erpx/server-workflow/adapters/approvals";
