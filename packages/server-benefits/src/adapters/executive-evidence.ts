import {metricCatalogFor} from "@erpx/server-runtime/public/executive/metrics";
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {currentAuthority} from "@erpx/server-runtime/public/executive/access";
import {sha256} from "@erpx/server-runtime/public/crypto";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertProductionModuleEnabled} from "@erpx/server-pms/public/hospitality-rollout";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createBenefitEvidenceApplication,type BenefitEvidencePorts} from "@erpx/server-benefits/application/executive-evidence";
import {benefitEvidenceProvider,benefitEvidenceFields,type BenefitEvidenceRow} from "@erpx/server-benefits/domain/executive-evidence";
import {benefitMetricHistoryAccess,benefitEvidenceSourceScope} from "@erpx/server-benefits/adapters/record-purchases";
function application(database:SqlDatabase){
 const ports:BenefitEvidencePorts={clock:{now:Date.now},digest:sha256,
  async authorize(actor,metric){await currentAuthority(database,actor);await assertProductionModuleEnabled(database,actor.companyId,"benefits");if(!(await metricCatalogFor(database,actor)).some(item=>item.id===metric&&item.available)||!(await benefitMetricHistoryAccess(database,actor))[metric])throw new ApiError(403,"EVIDENCE_FORBIDDEN","Original Benefits history and current source authority are required.");},
  async rows(actor,input,cursor,id){const election=input.metric==="active-benefit-elections",alias=election?"e":"p",scope=await benefitEvidenceSourceScope(actor,input.metric),args:(string|number|null)[]=[actor.companyId,...scope.args],clauses=[`${alias}.company_id=?`,scope.where,election?`${alias}.status<>'cancelled'`:`${alias}.status='published'`,`${alias}.effective_from<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')`,`(${alias}.effective_to IS NULL OR ${alias}.effective_to>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))`];if(input.query){clauses.push(`strpos(lower(${alias}.status||' '||${alias}.id),lower(?))>0`);args.push(input.query);}if(cursor){clauses.push(`${alias}.id>?`);args.push(cursor.id);}if(id){clauses.push(`${alias}.id=?`);args.push(id);}args.push(id?1:input.limit+1);return(await database.prepare(`SELECT ${alias}.id,${alias}.status,${alias}.version,${alias}.updated_at,${alias}.effective_from,${alias}.effective_to,${election?"'Benefit election'":"p.name"} label FROM ${election?"benefit_elections e":"benefit_plans p"} WHERE ${clauses.join(' AND ')} ORDER BY ${alias}.id LIMIT ?`).bind(...args).all<BenefitEvidenceRow>()).results;},
  async audit(actor,metric,action,rows,queryHash){if(!rows.length)return;await database.prepare(`INSERT INTO executive_evidence_access_events(id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at) SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,r.id,r.version,?::jsonb,?,? FROM jsonb_to_recordset(?::jsonb)r(id text,version bigint)`).bind(actor.companyId,actor.userId,actor.assignmentId,benefitEvidenceProvider,metric,action,JSON.stringify(benefitEvidenceFields),queryHash,Date.now(),JSON.stringify(rows)).run();},
 };return createBenefitEvidenceApplication(ports);
}
async function translated<Result>(run:()=>Promise<Result>){try{return await run();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function searchBenefitEvidence(database:SqlDatabase,actor:WorkContext,raw:unknown){return translated(()=>application(database).search(actor,raw));}
export function readBenefitEvidence(database:SqlDatabase,actor:WorkContext,metric:string,id:string,filters:unknown){return translated(()=>application(database).read(actor,metric,id,filters));}
