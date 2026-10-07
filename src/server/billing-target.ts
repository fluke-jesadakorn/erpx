/** Compatibility entry point; legacy implementation belongs to billing. */
export {calculateTargetBillingPeriod,readTargetBillingPeriod} from "@/server/modules/billing/adapters/billing-target";
export type {TargetBillingPeriodView,TargetMeasuredEvidence,TargetRatedLine} from "@/server/modules/billing/adapters/billing-target";
