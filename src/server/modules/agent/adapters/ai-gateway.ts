import {aiGatewayBootstrapSources} from '../../manage_erpx/public/ai-gateway-sources';
import {aiGatewayCompanySources} from '../../organization/public/ai-gateway-sources';
import 'server-only';
import {isTransactionalSqlDatabase,type SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {createAiGatewayApplication,type AiGatewayRepository,type AiGatewayReservationRepository} from '../application/gateway';
import type {AiGatewayFacts,AiProviderStatement,AiVerifiedCost,AiPolicyChange,SafeAiGatewayRow} from '../domain/gateway';
export type {SafeAiGatewayRow,AiGatewayPolicyView} from '../domain/gateway';
export type AiGatewayScope=AiGatewayFacts&{db:SqlDatabase};

function repository(db:SqlDatabase):AiGatewayRepository{const bootstrap=aiGatewayBootstrapSources(db),company=aiGatewayCompanySources(db);return {async policy(start:number,end:number){return db.prepare(`SELECT p.platform_enabled,p.monthly_request_limit,p.monthly_spend_cap_satang,p.reservation_satang,p.updated_by,p.updated_at,
    (SELECT count(*)::int FROM ai_gateway_requests r WHERE r.created_at>=? AND r.created_at<?) monthly_requests,
    (SELECT coalesce(sum(coalesce(e.thb_satang,r.reservation_satang)),0) FROM ai_gateway_requests r
      LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.created_at>=? AND r.created_at<?) monthly_reserved_or_verified_satang,
    (SELECT count(*)::int FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.status='succeeded' AND e.reconciled_at IS NULL
        AND r.created_at>=? AND r.created_at<?) pending_platform_cost_requests
    FROM ai_gateway_policies p WHERE p.singleton=1`)
    .bind(start,end,start,end,start,end)
    .first<{platform_enabled:boolean;monthly_request_limit:number;monthly_spend_cap_satang:number;reservation_satang:number;updated_by:string;updated_at:number;monthly_requests:number;monthly_reserved_or_verified_satang:number;pending_platform_cost_requests:number}>();},
async setEnabled(input:{enabled:boolean;actorId:string;reason:string},now:number,auditId:string){const results=await db.batch([
    db.prepare("UPDATE ai_gateway_policies SET platform_enabled=?,updated_by=?,updated_at=? WHERE singleton=1")
      .bind(input.enabled,input.actorId,now),
    db.prepare(`INSERT INTO ai_gateway_policy_audit(id,actor_id,reason,platform_enabled,monthly_request_limit,monthly_spend_cap_satang,reservation_satang,created_at)
      SELECT ?,?,?,p.platform_enabled,p.monthly_request_limit,p.monthly_spend_cap_satang,p.reservation_satang,?
      FROM ai_gateway_policies p WHERE p.singleton=1`)
      .bind(auditId,input.actorId,input.reason,now),
  ]);return results[0]?.meta.changes===1&&results[1]?.meta.changes===1;},
async statement(input:AiProviderStatement,now:number){const result=await db.prepare(`INSERT INTO ai_provider_statements(id,holding_id,provider,month,currency,amount_minor,statement_digest,verified_by,verified_at)
    SELECT ?,h.holding_id,?,?,?,?,?,?,? ${bootstrap.statementSource} ON CONFLICT(id) DO NOTHING`)
    .bind(input.id,input.provider,input.month,input.currency,input.amountMinor,input.statementDigest,input.verifiedBy,now,input.holdingId).run();return result.meta.changes===1;},
async statementEvidence(id:string){return db.prepare("SELECT holding_id,provider,month,currency,amount_minor,statement_digest FROM ai_provider_statements WHERE id=?")
    .bind(id).first<{holding_id:string;provider:string;month:string;currency:string;amount_minor:number;statement_digest:string}>();},
async reserve(scope:AiGatewayFacts,provider:string,model:string,now:number,start:number,end:number){const results=await db.batch([
    db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind(`ai-gateway:${scope.holdingId}`),
    db.prepare(`
      INSERT INTO ai_gateway_requests
        (id,usage_id,holding_id,company_id,branch_id,connection_id,funding_source,provider,model,feature,status,reservation_satang,created_at)
      SELECT ?,?,?,?,?,?,?,?, ?,?,'reserved',
        CASE WHEN ?='erpx' THEN p.reservation_satang ELSE 0 END,?
      FROM ai_gateway_policies p
      WHERE p.singleton=1 AND (${bootstrap.scopeFence}
        OR ${company.scopeFence})
        AND (p.platform_enabled OR ?<>'erpx')
        AND (SELECT count(*) FROM ai_gateway_requests r WHERE r.created_at>=? AND r.created_at<?)<p.monthly_request_limit
        AND (?<>'erpx' OR
          (SELECT coalesce(sum(coalesce(e.thb_satang,r.reservation_satang)),0)
             FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
            WHERE r.funding_source='erpx' AND r.created_at>=? AND r.created_at<?)
           +p.reservation_satang<=p.monthly_spend_cap_satang)
      ON CONFLICT (id) DO NOTHING`)
      .bind(scope.requestId,scope.usageId??scope.requestId,scope.holdingId,scope.companyId,scope.branchId,scope.connectionId,scope.fundingSource,provider,model,scope.feature,scope.fundingSource,now,scope.holdingId,scope.companyId,scope.companyId,scope.fundingSource,start,end,scope.fundingSource,start,end),
  ]);return results[1]?.meta.changes===1;},
async finish(scope:AiGatewayFacts,status:'succeeded'|'failed',inputTokens:number,outputTokens:number,units:Record<string,number>,now:number){const changed=await db.prepare("UPDATE ai_gateway_requests SET status=?,input_tokens=?,output_tokens=?,units_json=?::jsonb,completed_at=? WHERE id=? AND holding_id=? AND company_id=? AND status='reserved'")
    .bind(status,inputTokens,outputTokens,JSON.stringify(units),now,scope.requestId,scope.holdingId,scope.companyId).run();return changed.meta.changes===1;},
async costEvidence(input:AiVerifiedCost,now:number){const result=await db.prepare(`
    INSERT INTO ai_provider_cost_evidence(request_id,provider_statement_id,currency,amount_minor,thb_satang,fx_evidence_ref,verified_by,verified_at,reconciled_at)
    SELECT r.id,?,?,?,?,?,?,?,NULL
    FROM ai_gateway_requests r JOIN ai_provider_statements s ON s.id=?
    WHERE r.id=? AND r.status='succeeded' AND r.funding_source='erpx'
      AND s.holding_id=r.holding_id AND s.provider=r.provider AND s.currency=?
      AND s.month=to_char(to_timestamp(r.created_at/1000.0) AT TIME ZONE 'Asia/Bangkok','YYYY-MM')
      AND s.reconciled_at IS NULL
    ON CONFLICT (request_id) DO NOTHING`)
    .bind(input.providerStatementId,input.currency,input.amountMinor,input.thbSatang,input.fxEvidenceRef??null,input.verifiedBy,now,input.providerStatementId,input.requestId,input.currency).run();return result.meta.changes===1;},
async existingCost(id:string){return db.prepare("SELECT provider_statement_id,currency,amount_minor,thb_satang,fx_evidence_ref FROM ai_provider_cost_evidence WHERE request_id=?")
    .bind(id).first<{provider_statement_id:string;currency:string;amount_minor:number;thb_satang:number;fx_evidence_ref:string|null}>();},
async reconcile(statementId:string,now:number){const result=await db.batch([
    db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind(`ai-statement:${statementId}`),
    db.prepare(`UPDATE ai_provider_statements s SET reconciled_at=? WHERE s.id=? AND s.reconciled_at IS NULL
      AND s.amount_minor=(SELECT coalesce(sum(e.amount_minor),0) FROM ai_provider_cost_evidence e WHERE e.provider_statement_id=s.id)
      AND NOT EXISTS(SELECT 1 FROM ai_provider_cost_evidence e WHERE e.provider_statement_id=s.id AND e.currency<>s.currency)`)
      .bind(now,statementId),
    db.prepare(`UPDATE ai_provider_cost_evidence e SET reconciled_at=s.reconciled_at
      FROM ai_provider_statements s WHERE e.provider_statement_id=s.id AND s.id=? AND s.reconciled_at IS NOT NULL AND e.reconciled_at IS NULL`).bind(statementId),
  ]);return result[1]?.meta.changes===1;},
async existingReconciliation(statementId:string){return db.prepare("SELECT reconciled_at FROM ai_provider_statements WHERE id=?").bind(statementId).first<{reconciled_at:number|null}>();},
async safeRows(start:number,end:number){return (await db.prepare(`SELECT r.id,r.usage_id,r.holding_id,r.company_id,r.branch_id,r.connection_id,r.funding_source,r.provider,r.model,r.feature,r.status,
    r.input_tokens,r.output_tokens,r.units_json::text units_json,r.created_at,r.completed_at,
    e.provider_statement_id,e.currency,e.amount_minor,e.thb_satang,e.verified_at,e.reconciled_at
    FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
    WHERE r.created_at>=? AND r.created_at<? ORDER BY r.created_at,r.id`).bind(start,end).all<SafeAiGatewayRow>()).results;},
async updatePolicy(input:AiPolicyChange,now:number,auditId:string,auditNow:number){await db.batch([
    db.prepare("UPDATE ai_gateway_policies SET platform_enabled=?,monthly_request_limit=?,monthly_spend_cap_satang=?,reservation_satang=?,updated_by=?,updated_at=? WHERE singleton=1")
      .bind(input.enabled,input.monthlyRequestLimit,input.monthlySpendCapSatang,input.reservationSatang,input.actorId,now),
    db.prepare("INSERT INTO ai_gateway_policy_audit(id,actor_id,reason,platform_enabled,monthly_request_limit,monthly_spend_cap_satang,reservation_satang,created_at) VALUES(?,?,?,?,?,?,?,?)")
      .bind(auditId,input.actorId,input.reason,input.enabled,input.monthlyRequestLimit,input.monthlySpendCapSatang,input.reservationSatang,auditNow),
  ]);
},
async lockHolding(holdingId:string){await db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind('ai-gateway:'+holdingId).run();},
async reservationFacts(scope:AiGatewayFacts){const row=await db.prepare(`SELECT (${bootstrap.scopeFence} OR ${company.scopeFence}) scope_valid,EXISTS(SELECT 1 FROM ai_gateway_requests r WHERE r.id=?) already_reserved`).bind(scope.holdingId,scope.companyId,scope.companyId,scope.requestId).first<{scope_valid:boolean;already_reserved:boolean}>();return {scopeValid:row!.scope_valid,alreadyReserved:row!.already_reserved};}};}
function application(db:SqlDatabase){return createAiGatewayApplication({repository:repository(db),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},reservations:{run:<Result>(action:(r:AiGatewayReservationRepository)=>Promise<Result>)=>isTransactionalSqlDatabase(db)?db.transaction(tx=>action(repository(tx))):action(repository(db))}});}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function readAiGatewayPolicy(db: SqlDatabase,now=Date.now()){return exposed(()=>application(db).readAiGatewayPolicy(now));}
export async function setAiPlatformKeyEnabled(db: SqlDatabase,input: {enabled:boolean;actorId:string;reason:string}){return exposed(()=>application(db).setAiPlatformKeyEnabled(input));}
export async function recordAiProviderStatement(db: SqlDatabase,input: {
  id: string; holdingId: string; provider: string; month: string;
  currency: string; amountMinor: number; statementDigest: string; verifiedBy: string;
}){return exposed(()=>application(db).recordAiProviderStatement(input));}
export async function reserveAiGatewayRequest(scope: AiGatewayScope,provider: string,model: string){return exposed(()=>application(scope.db).reserveAiGatewayRequest(scope,provider,model));}
export async function finishAiGatewayRequest(scope: AiGatewayScope,status: "succeeded" | "failed",inputTokens: number,outputTokens: number,units: Record<string, number>){return exposed(()=>application(scope.db).finishAiGatewayRequest(scope,status,inputTokens,outputTokens,units));}
export async function recordVerifiedAiCost(db: SqlDatabase,input: {
  requestId: string; providerStatementId: string; currency: string;
  amountMinor: number; thbSatang: number; fxEvidenceRef?: string | null;
  verifiedBy: string;
}){return exposed(()=>application(db).recordVerifiedAiCost(input));}
export async function reconcileAiProviderStatement(db: SqlDatabase,statementId: string){return exposed(()=>application(db).reconcileAiProviderStatement(statementId));}
export async function safeAiGatewayRows(db: SqlDatabase,month: string){return exposed(()=>application(db).safeAiGatewayRows(month));}
export async function updateAiGatewayPolicy(db: SqlDatabase,input: {
  enabled: boolean; monthlyRequestLimit: number; monthlySpendCapSatang: number;
  reservationSatang: number; actorId: string; reason: string;
}){return exposed(()=>application(db).updateAiGatewayPolicy(input));}

