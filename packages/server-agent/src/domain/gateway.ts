export type AiGatewayFacts={holdingId:string;companyId:string;branchId:string|null;connectionId:string|null;fundingSource:'erpx'|'byok';feature:string;requestId:string;usageId?:string};

export type SafeAiGatewayRow = {
  id: string; usage_id: string; holding_id: string; company_id: string; branch_id: string | null;
  connection_id: string | null; funding_source: "erpx" | "byok";
  provider: string; model: string; feature: string;
  status: "reserved" | "succeeded" | "failed";
  input_tokens: number; output_tokens: number; units_json: string;
  created_at: number; completed_at: number | null;
  provider_statement_id: string | null; currency: string | null;
  amount_minor: number | null; thb_satang: number | null;
  verified_at: number | null; reconciled_at: number | null;
};

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
export type AiProviderStatement={
  id: string; holdingId: string; provider: string; month: string;
  currency: string; amountMinor: number; statementDigest: string; verifiedBy: string;
};
export type AiVerifiedCost={
  requestId: string; providerStatementId: string; currency: string;
  amountMinor: number; thbSatang: number; fxEvidenceRef?: string | null;
  verifiedBy: string;
};
export type AiPolicyChange={
  enabled: boolean; monthlyRequestLimit: number; monthlySpendCapSatang: number;
  reservationSatang: number; actorId: string; reason: string;
};
export type AiPolicyFacts={platform_enabled:boolean;monthly_request_limit:number;monthly_spend_cap_satang:number;reservation_satang:number;updated_by:string;updated_at:number;monthly_requests:number;monthly_reserved_or_verified_satang:number;pending_platform_cost_requests:number};
export function positiveInteger(value: number, name: string) {
  if (!Number.isSafeInteger(value) || value < 0) throw new BusinessRuleError(422, "AI_COST_INVALID", `${name} must be a nonnegative integer.`);
}
export function reservationAllowed(policy:AiPolicyFacts|null,funding:'erpx'|'byok',scopeValid:boolean,alreadyReserved:boolean){return Boolean(policy&&scopeValid&&!alreadyReserved&&(policy.platform_enabled||funding!=='erpx')&&policy.monthly_requests<policy.monthly_request_limit&&(funding!=='erpx'||policy.monthly_reserved_or_verified_satang+policy.reservation_satang<=policy.monthly_spend_cap_satang));}
