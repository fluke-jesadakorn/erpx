import { billingMonth, monthBounds } from "@/lib/billing";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";

export type AiGatewayScope = {
  db: SqlDatabase;
  holdingId: string;
  companyId: string;
  branchId: string | null;
  connectionId: string | null;
  fundingSource: "erpx" | "byok";
  feature: string;
  requestId: string;
  usageId?: string;
};

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

export async function readAiGatewayPolicy(db: SqlDatabase, now=Date.now()): Promise<AiGatewayPolicyView> {
  const {start,end}=monthBounds(billingMonth(now));
  const row=await db.prepare(`SELECT p.platform_enabled,p.monthly_request_limit,p.monthly_spend_cap_satang,p.reservation_satang,p.updated_by,p.updated_at,
    (SELECT count(*)::int FROM ai_gateway_requests r WHERE r.created_at>=? AND r.created_at<?) monthly_requests,
    (SELECT coalesce(sum(coalesce(e.thb_satang,r.reservation_satang)),0) FROM ai_gateway_requests r
      LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.created_at>=? AND r.created_at<?) monthly_reserved_or_verified_satang,
    (SELECT count(*)::int FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.status='succeeded' AND e.reconciled_at IS NULL
        AND r.created_at>=? AND r.created_at<?) pending_platform_cost_requests
    FROM ai_gateway_policies p WHERE p.singleton=1`)
    .bind(start,end,start,end,start,end)
    .first<{platform_enabled:boolean;monthly_request_limit:number;monthly_spend_cap_satang:number;reservation_satang:number;updated_by:string;updated_at:number;monthly_requests:number;monthly_reserved_or_verified_satang:number;pending_platform_cost_requests:number}>();
  if (!row) throw new ApiError(503,"AI_GATEWAY_POLICY_MISSING","The AI gateway policy is unavailable.");
  return {
    platformEnabled:row.platform_enabled,
    monthlyRequestLimit:row.monthly_request_limit,
    monthlySpendCapSatang:row.monthly_spend_cap_satang,
    reservationSatang:row.reservation_satang,
    monthlyRequests:row.monthly_requests,
    monthlyReservedOrVerifiedSatang:row.monthly_reserved_or_verified_satang,
    pendingPlatformCostRequests:row.pending_platform_cost_requests,
    updatedBy:row.updated_by,
    updatedAt:row.updated_at,
  };
}

export async function setAiPlatformKeyEnabled(db: SqlDatabase, input: {enabled:boolean;actorId:string;reason:string}) {
  if (!input.actorId.trim() || !input.reason.trim())
    throw new ApiError(422,"AI_GATEWAY_REASON","Record who changed the AI policy and why.");
  const now=Date.now();
  const results=await db.batch([
    db.prepare("UPDATE ai_gateway_policies SET platform_enabled=?,updated_by=?,updated_at=? WHERE singleton=1")
      .bind(input.enabled,input.actorId,now),
    db.prepare(`INSERT INTO ai_gateway_policy_audit(id,actor_id,reason,platform_enabled,monthly_request_limit,monthly_spend_cap_satang,reservation_satang,created_at)
      SELECT ?,?,?,p.platform_enabled,p.monthly_request_limit,p.monthly_spend_cap_satang,p.reservation_satang,?
      FROM ai_gateway_policies p WHERE p.singleton=1`)
      .bind(crypto.randomUUID(),input.actorId,input.reason,now),
  ]);
  if (results[0]?.meta.changes!==1 || results[1]?.meta.changes!==1)
    throw new ApiError(503,"AI_GATEWAY_POLICY_MISSING","The AI gateway policy is unavailable.");
}

export async function recordAiProviderStatement(db: SqlDatabase, input: {
  id: string; holdingId: string; provider: string; month: string;
  currency: string; amountMinor: number; statementDigest: string; verifiedBy: string;
}) {
  positiveInteger(input.amountMinor,"Statement amount");
  monthBounds(input.month);
  if (!input.id.trim() || !input.holdingId.trim() || !input.provider.trim() || !/^[A-Z]{3}$/.test(input.currency) || !/^[a-f0-9]{64}$/i.test(input.statementDigest) || !input.verifiedBy.trim())
    throw new ApiError(422,"AI_STATEMENT_INVALID","Provide the provider statement, its checksum and the verifier.");
  const result=await db.prepare(`INSERT INTO ai_provider_statements(id,holding_id,provider,month,currency,amount_minor,statement_digest,verified_by,verified_at)
    SELECT ?,h.holding_id,?,?,?,?,?,?,? FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=? ON CONFLICT(id) DO NOTHING`)
    .bind(input.id,input.provider,input.month,input.currency,input.amountMinor,input.statementDigest,input.verifiedBy,Date.now(),input.holdingId).run();
  if (result.meta.changes===1) return;
  const row=await db.prepare("SELECT holding_id,provider,month,currency,amount_minor,statement_digest FROM ai_provider_statements WHERE id=?")
    .bind(input.id).first<{holding_id:string;provider:string;month:string;currency:string;amount_minor:number;statement_digest:string}>();
  if (row?.holding_id===input.holdingId && row.provider===input.provider && row.month===input.month && row.currency===input.currency && row.amount_minor===input.amountMinor && row.statement_digest===input.statementDigest) return;
  throw new ApiError(409,"AI_STATEMENT_CONFLICT","This provider statement ID already has different evidence.");
}

function positiveInteger(value: number, name: string) {
  if (!Number.isSafeInteger(value) || value < 0) throw new ApiError(422, "AI_COST_INVALID", `${name} must be a nonnegative integer.`);
}

/** Reserve before network I/O. A duplicate ID never causes a second provider call. */
export async function reserveAiGatewayRequest(scope: AiGatewayScope, provider: string, model: string) {
  if (!scope.requestId || !scope.feature || !scope.companyId || !scope.holdingId || !provider || !model)
    throw new ApiError(422, "AI_GATEWAY_SCOPE", "The AI request is missing its billing scope.");
  const now = Date.now(), { start, end } = monthBounds(billingMonth(now));
  const results = await scope.db.batch([
    scope.db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind(`ai-gateway:${scope.holdingId}`),
    scope.db.prepare(`
      INSERT INTO ai_gateway_requests
        (id,usage_id,holding_id,company_id,branch_id,connection_id,funding_source,provider,model,feature,status,reservation_satang,created_at)
      SELECT ?,?,?,?,?,?,?,?, ?,?,'reserved',
        CASE WHEN ?='erpx' THEN p.reservation_satang ELSE 0 END,?
      FROM ai_gateway_policies p
      WHERE p.singleton=1 AND (EXISTS(SELECT 1 FROM beta_bootstrap h WHERE h.singleton=1 AND h.holding_id=?)
        OR EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=? AND n.kind='company' AND n.company_id=? AND n.status='active'))
        AND (p.platform_enabled OR ?<>'erpx')
        AND (SELECT count(*) FROM ai_gateway_requests r WHERE r.created_at>=? AND r.created_at<?)<p.monthly_request_limit
        AND (?<>'erpx' OR
          (SELECT coalesce(sum(coalesce(e.thb_satang,r.reservation_satang)),0)
             FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
            WHERE r.funding_source='erpx' AND r.created_at>=? AND r.created_at<?)
           +p.reservation_satang<=p.monthly_spend_cap_satang)
      ON CONFLICT (id) DO NOTHING`)
      .bind(scope.requestId,scope.usageId??scope.requestId,scope.holdingId,scope.companyId,scope.branchId,scope.connectionId,scope.fundingSource,provider,model,scope.feature,scope.fundingSource,now,scope.holdingId,scope.companyId,scope.companyId,scope.fundingSource,start,end,scope.fundingSource,start,end),
  ]);
  if (results[1]?.meta.changes !== 1)
    throw new ApiError(429, "AI_GATEWAY_BLOCKED", "AI is paused, its monthly limit was reached, or this request was already sent. Continue manually or ask an administrator to review the limit.");
}

export async function finishAiGatewayRequest(scope: AiGatewayScope, status: "succeeded" | "failed", inputTokens: number, outputTokens: number, units: Record<string, number>) {
  positiveInteger(inputTokens,"Input tokens"); positiveInteger(outputTokens,"Output tokens");
  const changed = await scope.db.prepare("UPDATE ai_gateway_requests SET status=?,input_tokens=?,output_tokens=?,units_json=?::jsonb,completed_at=? WHERE id=? AND holding_id=? AND company_id=? AND status='reserved'")
    .bind(status,inputTokens,outputTokens,JSON.stringify(units),Date.now(),scope.requestId,scope.holdingId,scope.companyId).run();
  if (changed.meta.changes !== 1)
    throw new ApiError(409,"AI_GATEWAY_STATE","The AI request changed while its provider result was being recorded.");
}

/** Only an independently verified, reconciled provider statement can supply billable cost. */
export async function recordVerifiedAiCost(db: SqlDatabase, input: {
  requestId: string; providerStatementId: string; currency: string;
  amountMinor: number; thbSatang: number; fxEvidenceRef?: string | null;
  verifiedBy: string;
}) {
  positiveInteger(input.amountMinor,"Provider amount"); positiveInteger(input.thbSatang,"THB amount");
  if (!/^[A-Z]{3}$/.test(input.currency) || !input.providerStatementId.trim() || !input.verifiedBy.trim() || (input.currency!=="THB" && !input.fxEvidenceRef?.trim()))
    throw new ApiError(422,"AI_COST_EVIDENCE_INVALID","Provide statement, currency, verifier and FX evidence for non-THB cost.");
  const now=Date.now();
  const result=await db.prepare(`
    INSERT INTO ai_provider_cost_evidence(request_id,provider_statement_id,currency,amount_minor,thb_satang,fx_evidence_ref,verified_by,verified_at,reconciled_at)
    SELECT r.id,?,?,?,?,?,?,?,NULL
    FROM ai_gateway_requests r JOIN ai_provider_statements s ON s.id=?
    WHERE r.id=? AND r.status='succeeded' AND r.funding_source='erpx'
      AND s.holding_id=r.holding_id AND s.provider=r.provider AND s.currency=?
      AND s.month=to_char(to_timestamp(r.created_at/1000.0) AT TIME ZONE 'Asia/Bangkok','YYYY-MM')
      AND s.reconciled_at IS NULL
    ON CONFLICT (request_id) DO NOTHING`)
    .bind(input.providerStatementId,input.currency,input.amountMinor,input.thbSatang,input.fxEvidenceRef??null,input.verifiedBy,now,input.providerStatementId,input.requestId,input.currency).run();
  if (result.meta.changes===1) return;
  const existing=await db.prepare("SELECT provider_statement_id,currency,amount_minor,thb_satang,fx_evidence_ref FROM ai_provider_cost_evidence WHERE request_id=?")
    .bind(input.requestId).first<{provider_statement_id:string;currency:string;amount_minor:number;thb_satang:number;fx_evidence_ref:string|null}>();
  if (existing && existing.provider_statement_id===input.providerStatementId && existing.currency===input.currency && existing.amount_minor===input.amountMinor && existing.thb_satang===input.thbSatang && existing.fx_evidence_ref===(input.fxEvidenceRef??null)) return;
  throw new ApiError(409,"AI_COST_EVIDENCE_CONFLICT","Cost evidence is missing, not billable, or already verified with different values.");
}

export async function reconcileAiProviderStatement(db: SqlDatabase, statementId: string) {
  const now=Date.now();
  const result=await db.batch([
    db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind(`ai-statement:${statementId}`),
    db.prepare(`UPDATE ai_provider_statements s SET reconciled_at=? WHERE s.id=? AND s.reconciled_at IS NULL
      AND s.amount_minor=(SELECT coalesce(sum(e.amount_minor),0) FROM ai_provider_cost_evidence e WHERE e.provider_statement_id=s.id)
      AND NOT EXISTS(SELECT 1 FROM ai_provider_cost_evidence e WHERE e.provider_statement_id=s.id AND e.currency<>s.currency)`)
      .bind(now,statementId),
    db.prepare(`UPDATE ai_provider_cost_evidence e SET reconciled_at=s.reconciled_at
      FROM ai_provider_statements s WHERE e.provider_statement_id=s.id AND s.id=? AND s.reconciled_at IS NOT NULL AND e.reconciled_at IS NULL`).bind(statementId),
  ]);
  if (result[1]?.meta.changes!==1) {
    const existing=await db.prepare("SELECT reconciled_at FROM ai_provider_statements WHERE id=?").bind(statementId).first<{reconciled_at:number|null}>();
    if (!existing?.reconciled_at) throw new ApiError(409,"AI_STATEMENT_UNBALANCED","Provider request costs do not match the statement total.");
  }
}

/** Safe for Control Plane projection: no prompt, answer, API key or business content. */
export async function safeAiGatewayRows(db: SqlDatabase, month: string): Promise<SafeAiGatewayRow[]> {
  const {start,end}=monthBounds(month);
  return (await db.prepare(`SELECT r.id,r.usage_id,r.holding_id,r.company_id,r.branch_id,r.connection_id,r.funding_source,r.provider,r.model,r.feature,r.status,
    r.input_tokens,r.output_tokens,r.units_json::text units_json,r.created_at,r.completed_at,
    e.provider_statement_id,e.currency,e.amount_minor,e.thb_satang,e.verified_at,e.reconciled_at
    FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
    WHERE r.created_at>=? AND r.created_at<? ORDER BY r.created_at,r.id`).bind(start,end).all<SafeAiGatewayRow>()).results;
}

export async function updateAiGatewayPolicy(db: SqlDatabase, input: {
  enabled: boolean; monthlyRequestLimit: number; monthlySpendCapSatang: number;
  reservationSatang: number; actorId: string; reason: string;
}) {
  for (const [name,value] of [["Request limit",input.monthlyRequestLimit],["Spend cap",input.monthlySpendCapSatang],["Reservation",input.reservationSatang]] as const) {
    positiveInteger(value,name);
    if (value===0) throw new ApiError(422,"AI_GATEWAY_POLICY","Gateway limits must be positive.");
  }
  if (!input.actorId.trim() || !input.reason.trim()) throw new ApiError(422,"AI_GATEWAY_REASON","Record who changed the AI policy and why.");
  await db.batch([
    db.prepare("UPDATE ai_gateway_policies SET platform_enabled=?,monthly_request_limit=?,monthly_spend_cap_satang=?,reservation_satang=?,updated_by=?,updated_at=? WHERE singleton=1")
      .bind(input.enabled,input.monthlyRequestLimit,input.monthlySpendCapSatang,input.reservationSatang,input.actorId,Date.now()),
    db.prepare("INSERT INTO ai_gateway_policy_audit(id,actor_id,reason,platform_enabled,monthly_request_limit,monthly_spend_cap_satang,reservation_satang,created_at) VALUES(?,?,?,?,?,?,?,?)")
      .bind(crypto.randomUUID(),input.actorId,input.reason,input.enabled,input.monthlyRequestLimit,input.monthlySpendCapSatang,input.reservationSatang,Date.now()),
  ]);
}
