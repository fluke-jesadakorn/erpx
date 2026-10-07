/** Compatibility entry point; legacy implementation belongs to people_payroll. */
export {administerLeave,createSelfServiceLeave,editSelfServiceLeave,leaveAdminSchema,leaveAdministration,leaveCalendarMonth,leaveEditSchema,leaveListFilterSchema,leaveSummary,listOwnLeave,listOwnLeavePage,normalizeLegacyLeave,previewLeave,runLeaveAccruals} from "@/server/modules/people_payroll/adapters/leave-self-service";
export type {LeaveCalendarMonth} from "@/server/modules/people_payroll/adapters/leave-self-service";
