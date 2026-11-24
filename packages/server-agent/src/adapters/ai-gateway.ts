import {aiGatewayBootstrapSources} from '@erpx/server-manage_erpx/public/ai-gateway-sources';
import {aiGatewayCompanySources} from '@erpx/server-organization/public/ai-gateway-sources';
import 'server-only';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
import {createAiGatewayApplication,type AiGatewayRepository,type AiGatewayReservationRepository} from '@erpx/server-agent/application/gateway';
import type {AiGatewayFacts,AiPolicyFacts} from '@erpx/server-agent/domain/gateway';
import {NativeQuotaError,nativeFinishQuota,nativeReserveQuota} from './native-quota-command';
export type {AiGatewayPolicyView} from '@erpx/server-agent/domain/gateway';
export type AiGatewayScope=AiGatewayFacts&{db:SqlDatabase};
/** Byte-identical Agent gateway monthly-request count projection shared by monthly_requests and pending_platform_cost_requests.
 * Each call site keeps its own binds; the native quota owner remains the authority. */
const aiGatewayRequestCountSql = '(SELECT count(*)::bigint FROM ai_gateway_requests r';

/**
 * Retained reservation/finish fence for the live `ai-transport` consumer.
 * Policy, statement, cost and reconciliation writes are already native. The
 * `ai_gateway_requests` reservation/completion writes are now owned by the
 * native Agent axum command: this adapter keeps only the in-process policy
 * reads and the reservation-allowed decision, then calls the owner route.
 * No TypeScript SQL writer and no fallback remain for these two writes.
 */
function repository(db:SqlDatabase):AiGatewayRepository{
  const bootstrap=aiGatewayBootstrapSources(db),company=aiGatewayCompanySources(db);
  const policyQuery=async(start:number,end:number)=>db.prepare(`SELECT p.platform_enabled,p.monthly_request_limit::bigint monthly_request_limit,p.monthly_spend_cap_satang,p.reservation_satang,p.updated_by,p.updated_at,
    ${aiGatewayRequestCountSql} WHERE r.created_at>=? AND r.created_at<?) monthly_requests,
    (SELECT coalesce(sum(coalesce(e.thb_satang,r.reservation_satang)),0)::bigint FROM ai_gateway_requests r
      LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.created_at>=? AND r.created_at<?) monthly_reserved_or_verified_satang,
    ${aiGatewayRequestCountSql} LEFT JOIN ai_provider_cost_evidence e ON e.request_id=r.id
      WHERE r.funding_source='erpx' AND r.status='succeeded' AND e.reconciled_at IS NULL
        AND r.created_at>=? AND r.created_at<?) pending_platform_cost_requests
    FROM ai_gateway_policies p WHERE p.singleton=1`)
    .bind(start,end,start,end,start,end)
    .first<AiPolicyFacts>();
  const repo:AiGatewayRepository={
    policy:policyQuery,
    async reserve(scope:AiGatewayFacts,provider:string,model:string,now:number,start:number,end:number){
      // Native axum command owns the reservation row and the per-Company lock.
      // No reservation write statement remains in TypeScript.
      const policy=await policyQuery(start,end);
      if(!policy)return false;
      await nativeReserveQuota(scope,{
        provider,model,now,windowStart:start,windowEnd:end,
        monthlyRequestLimit:Number(policy.monthly_request_limit),
        reservationSatang:policy.reservation_satang,
      },`ai-quota:${scope.requestId}`);
      return true;
    },
    async finish(scope:AiGatewayFacts,status:'succeeded'|'failed',inputTokens:number,outputTokens:number,units:Record<string,number>,now:number){
      // Native axum command owns the completion transition. No completion
      // write statement remains in TypeScript.
      await nativeFinishQuota(scope,{
        outcome:status,inputTokens,outputTokens,units,now,
      },`ai-quota-finish:${scope.requestId}`);
      return true;
    },
    async lockHolding(holdingId:string){await db.prepare("SELECT pg_advisory_xact_lock(hashtext(?))").bind('ai-gateway:'+holdingId).run();},
    async reservationFacts(scope:AiGatewayFacts){const row=await db.prepare(`SELECT (${bootstrap.scopeFence} OR ${company.scopeFence}) scope_valid,EXISTS(SELECT 1 FROM ai_gateway_requests r WHERE r.id=?) already_reserved`).bind(scope.holdingId,scope.companyId,scope.companyId,scope.requestId).first<{scope_valid:boolean;already_reserved:boolean}>();return {scopeValid:row!.scope_valid,alreadyReserved:row!.already_reserved};},
  };
  return repo;
}
function application(db:SqlDatabase){return createAiGatewayApplication({repository:repository(db),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},reservations:{run:<Result>(action:(r:AiGatewayReservationRepository)=>Promise<Result>)=>isTransactionalSqlDatabase(db)?db.transaction(tx=>action(repository(tx))):action(repository(db))}});}
async function exposed<T>(action:()=>Promise<T>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);if(error instanceof NativeQuotaError)throw new ApiError(error.status??503,"NATIVE_AGENT_QUOTA_UNAVAILABLE","The native Agent quota service is unavailable.");throw error;}}
export async function readAiGatewayPolicy(db: SqlDatabase,now=Date.now()){return exposed(()=>application(db).readAiGatewayPolicy(now));}
export async function reserveAiGatewayRequest(scope: AiGatewayScope,provider: string,model: string){return exposed(()=>application(scope.db).reserveAiGatewayRequest(scope,provider,model));}
export async function finishAiGatewayRequest(scope: AiGatewayScope,status: "succeeded" | "failed",inputTokens: number,outputTokens: number,units: Record<string, number>){return exposed(()=>application(scope.db).finishAiGatewayRequest(scope,status,inputTokens,outputTokens,units));}
