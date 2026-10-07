/** Compatibility entry point; legacy implementation belongs to people_payroll. */
export {assertPayrollSourceFresh,drainPayrollScheduledPreviews,drainPayrollSchedules,getPayrollSchedule,payrollReadiness,payrollScheduleSchema,payrollSourceGuards,payrollSubmitReviewSchema,savePayrollSchedule,submitPayrollReview} from "@/server/modules/people_payroll/adapters/payroll-readiness";
export type {PayrollBlocker} from "@/server/modules/people_payroll/adapters/payroll-readiness";
