/** Compatibility entry point; legacy implementation belongs to people_payroll. */
export {commandOvertime,createOvertime,getOvertime,listOvertime,overtimeSetup,overtimeWork,previewOvertime,setOvertimeEligibility} from "@/server/modules/people_payroll/adapters/overtime";
export type {OvertimeDetail,OvertimeRow} from "@/server/modules/people_payroll/adapters/overtime";
