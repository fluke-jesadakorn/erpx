/** Compatibility entry point; legacy implementation belongs to workflow. */
export {approvalView,canOverrideStaffStep,closeApprovals,currentApproval,initializeLegacyApproval,openApproval,participantSql,prepareDecision,rerouteApproval,staffOverrideGuards,stepBlock,taskRoute} from "@/server/modules/workflow/adapters/approvals";
export type {ApprovalDecision,ApprovalInstance,ApprovalKind,ApprovalPhase,ApprovalStep,ApprovalView} from "@/server/modules/workflow/adapters/approvals";
