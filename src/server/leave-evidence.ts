/** Compatibility entry point; legacy implementation belongs to people_payroll. */
export {assertLeaveAttachmentAccess,availableLeaveEvidence,leaveEvidenceManifest,leaveEvidencePurposes,parseLeaveEvidencePolicy,pendingLeaveEvidenceCount} from "@/server/modules/people_payroll/adapters/leave-evidence";
export type {LeaveEvidencePolicy,LeaveEvidencePurpose} from "@/server/modules/people_payroll/adapters/leave-evidence";
