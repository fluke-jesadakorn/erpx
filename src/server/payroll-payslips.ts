/** Compatibility entry point; legacy implementation belongs to people_payroll. */
export {getPayslip,getPayslipExport,issuePayslipSchema,issuePostedPayslips,listPayslips,preparePayslipIssue} from "@/server/modules/people_payroll/adapters/payroll-payslips";
export type {PayslipSnapshot} from "@/server/modules/people_payroll/adapters/payroll-payslips";
