import {aiGatewayBootstrapSources} from '@erpx/server-manage_erpx/public/ai-gateway-sources';
import {aiGatewayCompanySources} from '@erpx/server-organization/public/ai-gateway-sources';
import 'server-only';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createAiGatewayApplication,type AiGatewayRepository,type AiGatewayReservationRepository} from '@erpx/server-agent/application/gateway';
import type {AiGatewayFacts,AiPolicyFacts} from '@erpx/server-agent/domain/gateway';
export type {AiGatewayPolicyView} from '@erpx/server-agent/domain/gateway';
export type AiGatewayScope=AiGatewayFacts&{db:SqlDatabase};

/**
 * Retained reservation/finish ledger for the live `ai-transport` consumer.
 * Policy/statement/cost/reconciliation writes are native in
 * `erpx-server-agent`; this adapter only owns the in-process reservation fence.
 */
function repository(db:SqlDatabase):AiGatewayRepository{const bootstrap=aiGatewayBootstrapSources(db),company=aiGatewayCompanySources(db);return {async policy(start:number,end:number){return db.prepare(`SELECT p.platform_enabled,p.monthly_request_limit::bigint monthly_request_limit,p.monthly_spend_cap_satang,p.reservation_satang,p.updated_by,p.updated_at,
    (SELECT count(*)::bigint FROM ai_gateway_requests r WHERE r.created_at>=? AND r.created_at<?) monthly_requests,
    (SELECT coalesce(sum(coalesce(e.thb_satang,r.reservation_satang)),0)::bigint FROM ai_gateway_requests r
      LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.created_at>=? AND r.created_at<?) monthly_reserved_or_verified_satang,
    (SELECT count(*)::bigint FROM ai_gateway_requests r LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.status='succeeded' AND e.reconciled_at IS NULL
        AND r.created_at>=? AND r.created_at<?) pending_platform_cost_requests
    FROM ai_gateway_policies p WHERE p.singleton=1`)
    .bind(start,end,start,end,start,end)
    .first<AiPolicyFacts>();},
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
async lockHolding(holdingId:string){await db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind('ai-gateway:'+holdingId).run();},
async reservationFacts(scope:AiGatewayFacts){const row=await db.prepare(`SELECT (${bootstrap.scopeFence} OR ${company.scopeFence}) scope_valid,EXISTS(SELECT 1 FROM ai_gateway_requests r WHERE r.id=?) already_reserved`).bind(scope.holdingId,scope.companyId,scope.companyId,scope.requestId).first<{scope_valid:boolean;already_reserved:boolean}>();return {scopeValid:row!.scope_valid,alreadyReserved:row!.already_reserved};}};}
function application(db:SqlDatabase){return createAiGatewayApplication({repository:repository(db),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},reservations:{run:<Result>(action:(r:AiGatewayReservationRepository)=>Promise<Result>)=>isTransactionalSqlDatabase(db)?db.transaction(tx=>action(repository(tx))):action(repository(db))}});}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function readAiGatewayPolicy(db: SqlDatabase,now=Date.now()){return exposed(()=>application(db).readAiGatewayPolicy(now));}
export async function reserveAiGatewayRequest(scope: AiGatewayScope,provider: string,model: string){return exposed(()=>application(scope.db).reserveAiGatewayRequest(scope,provider,model));}
export async function finishAiGatewayRequest(scope: AiGatewayScope,status: "succeeded" | "failed",inputTokens: number,outputTokens: number,units: Record<string, number>){return exposed(()=>application(scope.db).finishAiGatewayRequest(scope,status,inputTokens,outputTokens,units));}
