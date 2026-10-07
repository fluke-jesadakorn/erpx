/** Compatibility entry point; legacy implementation belongs to people_payroll. */
export {approveLeave,assertLeaveAttachmentAccess,createLeave,decideLeaveCancellation,getLeave,leaveDecisionSchema,leaveDraftSchema,leaveLifecycleSchema,newLeaveSchema,submitLeave,transitionLeave} from "@/server/modules/people_payroll/adapters/leave";
export type {LeaveRequest} from "@/server/modules/people_payroll/adapters/leave";
