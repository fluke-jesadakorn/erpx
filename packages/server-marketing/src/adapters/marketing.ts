import { nativeCreateCandidate, nativeRecordEngagement, nativeReviewAsset } from '@erpx/feature-marketing/adapters/server/native-marketing-wave2';
import {filterMarketingPurchasedWork} from "@erpx/server-marketing/adapters/record-purchases";
import {marketingEvidenceCandidateRows,marketingEvidenceEventRows} from "./executive-evidence-rows";
import {marketingCandidateFormRows} from "./candidate-form-rows";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";

import "server-only";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";

import { marketingCampaigns, marketingCostSources } from "@erpx/server-marketing/public/marketing-campaigns";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {marketingFormReadiness,marketingPresentationAssertions} from "@erpx/server-marketing/adapters/public-form";
import {activeFormSql} from "./active-form-sql";
import {marketingCompanyListSql} from "./marketing-company-list-sql";
import {marketingLocalPointReadSql} from "./marketing-local-point-read-sql";
import {
  MARKETING_RD_ASSET_LIST,
  MARKETING_RD_ASSET_POINT,
  MARKETING_RD_FORM_SOURCE,
  nativeMarketingReadAll,
  nativeMarketingReadConfigured,
  nativeMarketingReadFirst,
} from "./native-marketing-read";
import {listAttachmentPublications} from "@erpx/server-files/public/attachments/service";
import {attachmentsV2Readable} from "@erpx/server-files/public/attachments/feature";
import {isTransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createCRMLeadInTransaction} from "@erpx/server-crm/public/lead-handoff";
import {crmMarketingCandidateTransaction} from "@erpx/server-crm/public/purchases";
import {marketingActionSchema,marketingCandidateSchema} from "@erpx/server-marketing/domain/marketing";
import {requireMarketingPurchasedRead,marketingSourceAccess,marketingCandidateReadFilter,marketingFinanceHistory,marketingHandoffControls,withMarketingPurchasedCommand} from "@erpx/server-marketing/adapters/purchases";
import {createMarketingApplication,type MarketingRepository,type MarketingMutation} from "@erpx/server-marketing/application/marketing";
export type {MarketingExecutiveEvidenceMetric} from "@erpx/server-marketing/domain/marketing";
import type {MarketingExecutiveEvidenceMetric} from "@erpx/server-marketing/domain/marketing";
export {createMarketingAsset} from "@erpx/server-marketing/adapters/asset";
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "marketing"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Marketing.",
    );
}

function repository(write:SqlDatabase,a:WorkContext,startTransaction:boolean):MarketingRepository<SqlStatement[]>{return {
...(startTransaction&&isTransactionalSqlDatabase(write)?{transaction:<Result>(action:(r:MarketingRepository<SqlStatement[]>)=>Promise<Result>)=>write.transaction(tx=>action(repository(tx,a,false)))}:{}),
async assets(){
      if(nativeMarketingReadConfigured())return (await nativeMarketingReadAll<Row>(MARKETING_RD_ASSET_LIST,{companyId:a.companyId},write)).results;
      return (await write
        .prepare(
          marketingCompanyListSql('marketing_assets',''),
        )
        .bind(a.companyId)
        .all<Row>()).results;},
async candidates(){return (await write
        .prepare(
          `${marketingCandidateFormRows()} WHERE c.company_id=? AND ${await marketingCandidateReadFilter('c')} AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY c.updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()).results;},
async branches(){return (await write
        .prepare(
          `SELECT id,display_name name FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name`,
        )
        .bind(a.companyId, a.branchId, a.branchId)
        .all<Row>()).results;},
async campaignSummary(){return (await write
        .prepare(
          `SELECT COALESCE(NULLIF(BTRIM(attribution_json::jsonb->>'campaign'),''),'Unspecified') campaign,
            COALESCE(NULLIF(BTRIM(attribution_json::jsonb->>'source'),''),NULLIF(BTRIM(attribution_json::jsonb->>'utmSource'),''),'Unspecified') source,
            COUNT(*)::int captured,
            COUNT(*) FILTER (WHERE status='pending')::int pending,
            COUNT(*) FILTER (WHERE status='handed_off')::int handed_off,
            COUNT(*) FILTER (WHERE status='rejected')::int rejected
          FROM marketing_lead_candidates
          WHERE company_id=? AND ${await marketingCandidateReadFilter('marketing_lead_candidates')} AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)
          GROUP BY 1,2 ORDER BY COUNT(*) DESC, campaign, source`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()).results;},
async sourceForm(formId,formVersion){
      if(nativeMarketingReadConfigured())return nativeMarketingReadFirst<{configuration_json:string}>(MARKETING_RD_FORM_SOURCE,{assetId:formId,companyId:a.companyId,version:formVersion},write);
      return write.prepare(activeFormSql('source',true,true)).bind(formId,a.companyId,formVersion).first<{configuration_json:string}>();},
async asset(id){
      if(nativeMarketingReadConfigured())return nativeMarketingReadFirst<Row>(MARKETING_RD_ASSET_POINT,{assetId:id,companyId:a.companyId},write);
      return write
        .prepare(marketingLocalPointReadSql('asset'))
        .bind(id, a.companyId)
        .first<Row>();},
candidate:(id,lock=false)=>write
    .prepare(
      marketingLocalPointReadSql('candidate',lock),
    )
    .bind(id, a.companyId)
    .first<Row>(),
detail:id=>write
      .prepare(
        `${marketingCandidateFormRows()} WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)`,
      )
      .bind(
        id,
        a.companyId,
        a.branchId,
        a.branchId,
        a.departmentId,
        a.departmentId,
      )
      .first<Row>(),
async history(id){return (await write
        .prepare(
          marketingLocalPointReadSql('history'),
        )
        .bind(id)
        .all<Row>()).results;},
async workRows(){return (await write
        .prepare(
          marketingCompanyListSql('marketing_lead_candidates',` AND ${await marketingCandidateReadFilter('marketing_lead_candidates')} AND status='pending' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)`),
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()).results;},
async evidence(metric,recordId,range){if(metric==="marketing-lead-candidates")return write.prepare(`${marketingEvidenceCandidateRows('marketing_lead_candidates')}
      WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) LIMIT 1`)
      .bind(recordId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId).first<Row>();if(!range)return null;return write.prepare(`${marketingEvidenceEventRows('marketing_lead_candidates')}
    WHERE e.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)
      AND e.created_at>=? AND e.created_at<? LIMIT 1`)
    .bind(recordId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,range.from,range.toExclusive).first<Row>();},

campaigns:()=>marketingCampaigns(a),costSources:()=>marketingCostSources(a),formReadiness:configuration=>marketingFormReadiness(write,a.companyId,configuration),publications:async(id)=>(await listAttachmentPublications(write,a,'marketing_asset',id)).items,preparePresentation:(key,configuration)=>marketingPresentationAssertions(write,key,a.companyId,configuration),
replay:(key,operation,id,input)=>replayCommand(write,a,key,operation,id,input),createCRMLead:(raw,key)=>createCRMLeadInTransaction(write,a,raw,key),commit:(mutation,result)=>commit(write,a,mutation,result),
};}
function application(a:WorkContext,provided?:SqlDatabase,now?:number){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createMarketingApplication({actorId:a.userId,clock:{now:()=>now??Date.now()},canRead:hasModuleAccess(a.modules,'marketing')&&hasPermission(a,'records.read'),canLinkCost:hasPermission(a,'finance.read')&&hasPermission(a,'records.write'),publicationsReadable:attachmentsV2Readable(),authorize:write=>access(a,write),repository:async()=>repository(await database(),a,!provided)});}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:MarketingMutation<SqlStatement[]>,result:Result):Promise<Result>{
  const {id,key}=m;
  switch(m.action){
    case "candidate": await nativeCreateCandidate(a,m.input,key); return result;
    case "asset": await nativeReviewAsset(a,id,m.input,key); return result;
    case "engagement": await nativeRecordEngagement(a,id,m.input,key); return result;
    case "handoff": throw new ApiError(503,'MARKETING_COMMAND_UNAVAILABLE','Candidate CRM handoff is not available on the native Marketing path.');
  }
}

async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function marketingWorkspace(a:WorkContext){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireMarketingPurchasedRead(db,a),view=await application(a,db).workspace();return {...view,...(purchase?{purchase,canWrite:hasPermission(a,'records.write'),costHistory:await marketingFinanceHistory(db,a)}: {})};});}
export function createMarketingCandidate(a:WorkContext,raw:unknown,key:string){return translated(()=>{const input=marketingCandidateSchema.parse(raw);return withMarketingPurchasedCommand(a,{key,operation:'marketing.candidate.create',target:{kind:'candidate',id:`marketing-candidate-${key}`},purpose:'new-work',sources:[{kind:'asset',id:input.formId}]},db=>application(a,db).createCandidate(input,key));});}
export function changeMarketing(a:WorkContext,kind:"asset"|"candidate",id:string,raw:unknown,key:string){return translated(()=>{const input=marketingActionSchema.parse(raw);return kind==='candidate'&&input.action==='handoff'?crmMarketingCandidateTransaction(a,id,input,key,(db,now)=>application(a,db,now).change(kind,id,input,key,repository(db,a,false))):withMarketingPurchasedCommand(a,{key,operation:kind==='asset'?`marketing.asset.${input.action}`:'marketing.candidate.engagement',target:{kind,id},existing:true,purpose:input.action==='activate'?'new-work':'settle-existing'},db=>application(a,db).change(kind,id,input,key));});}
export function marketingDetail(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireMarketingPurchasedRead(db,a,{kind:'candidate',id}),view=await application(a,db).detail(id),handoff=purchase?await marketingHandoffControls(db,a):undefined;return {...view,...(purchase?{purchase,canWrite:hasPermission(a,'records.write'),canHandoff:!!handoff?.sales&&hasPermission(a,'records.write')&&view.candidate.created_by!==a.userId,handoffPurchase:handoff?.purchase}: {})};});}
export function marketingExecutiveEvidenceRow(db:SqlDatabase,a:WorkContext,metric:MarketingExecutiveEvidenceMetric,id:string,range?:{from:number;toExclusive:number}){return translated(async()=>{await requireMarketingPurchasedRead(db,a);return application(a,db).evidence(metric,id,range);});}
export function marketingWork(a:WorkContext){return translated(async()=>{if(!hasModuleAccess(a.modules,'marketing')||!hasPermission(a,'records.read'))return [];const db=(await holdingResources(a.holdingId)).write,purchase=await marketingSourceAccess(db,a);if(purchase&&!purchase.history)return [];const rows=await application(a,db).work();return filterMarketingPurchasedWork(db,a,purchase&&!purchase.settleExisting?rows.map(row=>({...row,canAct:false,queue:'waiting' as const})):rows);});}
