export type AiGatewayFacts={holdingId:string;companyId:string;branchId:string|null;connectionId:string|null;fundingSource:'erpx'|'byok';feature:string;requestId:string;usageId?:string};

export type AiGatewayPolicyView = {
  platformEnabled: boolean;
  monthlyRequestLimit: number;
  monthlySpendCapSatang: number;
  reservationSatang: number;
  monthlyRequests: number;
  monthlyReservedOrVerifiedSatang: number;
  pendingPlatformCostRequests: number;
  updatedBy: string;
  updatedAt: number;
};


import {BusinessRuleError} from '@erpx/server-shared/index';
export type AiPolicyFacts={platform_enabled:boolean;monthly_request_limit:number;monthly_spend_cap_satang:number;reservation_satang:number;updated_by:string;updated_at:number;monthly_requests:number;monthly_reserved_or_verified_satang:number;pending_platform_cost_requests:number};
export function positiveInteger(value: number, name: string) {
  if (!Number.isSafeInteger(value) || value < 0) throw new BusinessRuleError(422, "AI_COST_INVALID", `${name} must be a nonnegative integer.`);
}
export function reservationAllowed(policy:AiPolicyFacts|null,funding:'erpx'|'byok',scopeValid:boolean,alreadyReserved:boolean){return Boolean(policy&&scopeValid&&!alreadyReserved&&(policy.platform_enabled||funding!=='erpx')&&policy.monthly_requests<policy.monthly_request_limit&&(funding!=='erpx'||policy.monthly_reserved_or_verified_satang+policy.reservation_satang<=policy.monthly_spend_cap_satang));}
