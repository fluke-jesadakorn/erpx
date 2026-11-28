/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { financeDueDraftSchema, financeDueDecisionSchema, financeDueWorkspace, scheduleFinanceDraft, decideFinanceDueDraft, drainFinanceDueDrafts } from "@erpx/finance-server/adapters/finance-due-drafts";
export { nativeFinanceDueDraftSchedule, nativeFinanceDueDraftDecide } from "../adapters/native-finance-due-draft";
