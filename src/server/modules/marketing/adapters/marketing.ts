import {filterMarketingPurchasedWork} from "./record-purchases";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";

import "server-only";

import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { Row } from "@/server/modules/commerce/public/commerce";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";

import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { hasModuleAccess } from "@/data/module-access";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";

import { marketingCampaigns, marketingCostSources } from "@/server/modules/marketing/public/marketing-campaigns";
import {hasPermission} from "@/lib/identity_access/permissions";
import {marketingFormReadiness,marketingPresentationAssertions} from "@/server/modules/marketing/adapters/public-form";
import {listAttachmentPublications} from "@/server/modules/files/public/attachments/service";
import {attachmentsV2Readable} from "@/server/modules/files/public/attachments/feature";
import {isTransactionalSqlDatabase} from "../../runtime/public/runtime/types";
import type {SqlStatement} from "../../runtime/public/runtime/types";
import {BusinessRuleError} from "../../shared";
import {createCRMLeadInTransaction} from "../../crm/public/lead-handoff";
import {crmMarketingCandidateTransaction} from "../../crm/public/purchases";
import {marketingActionSchema,marketingCandidateSchema} from "../domain/marketing";
import {requireMarketingPurchasedRead,marketingSourceAccess,marketingCandidateReadFilter,marketingFinanceHistory,marketingHandoffControls,withMarketingPurchasedCommand} from "./purchases";
import {createMarketingApplication,type MarketingRepository,type MarketingMutation} from "../application/marketing";
export type {MarketingExecutiveEvidenceMetric} from "../domain/marketing";
import type {MarketingExecutiveEvidenceMetric} from "../domain/marketing";
export {createMarketingAsset} from "./asset";
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
async assets(){return (await write
        .prepare(
          "SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC",
        )
        .bind(a.companyId)
        .all<Row>()).results;},
async candidates(){return (await write
        .prepare(
          `SELECT c.*,a.name form_name FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id WHERE c.company_id=? AND ${await marketingCandidateReadFilter('c')} AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) ORDER BY c.updated_at DESC`,
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
sourceForm:(formId,formVersion)=>write.prepare("SELECT configuration_json FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND kind='form' AND status='active'").bind(formId,a.companyId,formVersion).first<{configuration_json:string}>(),
asset:id=>write
      .prepare("SELECT * FROM marketing_assets WHERE id=? AND company_id=?")
      .bind(id, a.companyId)
      .first<Row>(),
candidate:(id,lock=false)=>write
    .prepare(
      `SELECT * FROM marketing_lead_candidates WHERE id=? AND company_id=? ${lock?"FOR UPDATE":""}`,
    )
    .bind(id, a.companyId)
    .first<Row>(),
detail:id=>write
      .prepare(
        "SELECT c.*,a.name form_name FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)",
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
          "SELECT * FROM marketing_engagement_events WHERE candidate_id=? ORDER BY version",
        )
        .bind(id)
        .all<Row>()).results;},
async workRows(){return (await write
        .prepare(
          `SELECT * FROM marketing_lead_candidates WHERE company_id=? AND ${await marketingCandidateReadFilter('marketing_lead_candidates')} AND status='pending' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()).results;},
async evidence(metric,recordId,range){if(metric==="marketing-lead-candidates")return write.prepare(`SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,
        f.name form_name
      FROM marketing_lead_candidates c
      JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
      WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?) LIMIT 1`)
      .bind(recordId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId).first<Row>();if(!range)return null;return write.prepare(`SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,
      c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name
    FROM marketing_engagement_events e
    JOIN marketing_lead_candidates c ON c.id=e.candidate_id
    JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
    WHERE e.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)
      AND e.created_at>=? AND e.created_at<? LIMIT 1`)
    .bind(recordId,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,range.from,range.toExclusive).first<Row>();},

campaigns:()=>marketingCampaigns(a),costSources:()=>marketingCostSources(a),formReadiness:configuration=>marketingFormReadiness(write,a.companyId,configuration),publications:async(id)=>(await listAttachmentPublications(write,a,'marketing_asset',id)).items,preparePresentation:(key,configuration)=>marketingPresentationAssertions(write,key,a.companyId,configuration),
replay:(key,operation,id,input)=>replayCommand(write,a,key,operation,id,input),createCRMLead:(raw,key)=>createCRMLeadInTransaction(write,a,raw,key),commit:(mutation,result)=>commit(write,a,mutation,result),
};}
function application(a:WorkContext,provided?:SqlDatabase,now?:number){let resource:Promise<SqlDatabase>|null=provided?Promise.resolve(provided):null;const database=()=>resource??=holdingResources(a.holdingId).then(x=>x.write);return createMarketingApplication({actorId:a.userId,clock:{now:()=>now??Date.now()},canRead:hasModuleAccess(a.modules,'marketing')&&hasPermission(a,'records.read'),canLinkCost:hasPermission(a,'finance.read')&&hasPermission(a,'records.write'),publicationsReadable:attachmentsV2Readable(),authorize:write=>access(a,write),repository:async()=>repository(await database(),a,!provided)});}
async function commit<Result>(write:SqlDatabase,a:WorkContext,m:MarketingMutation<SqlStatement[]>,result:Result):Promise<Result>{const {key,id,operation:op,now}=m;switch(m.action){
case "candidate":{const i=m.input;return executeCommand(write, a, key, op, i, id, 1, result, [
    ...(m.presentation??[]),
    assertion(
      write,
      key,
      "form",
      "EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='form' AND status='active' AND version=? AND configuration_json::jsonb->'consentRequired'='true'::jsonb)",
      [i.formId, a.companyId, i.formVersion],
    ),
    assertion(
      write,
      key,
      "branch",
      `EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?))`,
      [i.branchId, a.companyId, a.branchId, a.branchId],
    ),
    write
      .prepare(
        "INSERT INTO marketing_lead_candidates VALUES(?,?,?,?,?,?,?,?,?,?,?,'pending',NULL,?,NULL,1,?,?)",
      )
      .bind(
        id,
        a.companyId,
        i.branchId,
        a.departmentId,
        i.formId,
        i.formVersion,
        i.contactName,
        i.contactDetail,
        i.consentEvidence,
        JSON.stringify(i.attribution),
        i.brief,
        a.userId,
        now,
        now,
      ),
    write
      .prepare(
        "INSERT INTO marketing_engagement_events VALUES(?,?,1,'form_submit','web',?,?,?)",
      )
      .bind(
        `marketing-engagement-${key}`,
        id,
        i.consentEvidence,
        a.userId,
        now,
      ),
  ]);}
case "asset":{const i=m.input,expected=m.expected,status=m.status;return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      result,
      [
        ...(m.presentation??[]),
        assertion(
          write,
          key,
          "asset",
          "EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND version=? AND status=?)",
          [id, i.expectedVersion, expected],
        ),
        write
          .prepare(
            "UPDATE marketing_assets SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(status, a.userId, now, id),
        write
          .prepare("INSERT INTO marketing_asset_events VALUES(?,?,?,?,?,?,?,?)")
          .bind(
            `marketing-asset-event-${key}`,
            id,
            i.expectedVersion + 1,
            i.action,
            a.userId,
            i.reason,
            status,
            now,
          ),
      ],
    );}
case "engagement":{const i=m.input;return executeCommand(
      write,
      a,
      key,
      op,
      i,
      id,
      i.expectedVersion + 1,
      result,
      [
        assertion(
          write,
          key,
          "candidate",
          "EXISTS(SELECT 1 FROM marketing_lead_candidates WHERE id=? AND version=? AND status='pending')",
          [id, i.expectedVersion],
        ),
        write
          .prepare(
            "UPDATE marketing_lead_candidates SET version=version+1,updated_at=? WHERE id=?",
          )
          .bind(now, id),
        write
          .prepare(
            "INSERT INTO marketing_engagement_events VALUES(?,?,?,?,?,?,?,?)",
          )
          .bind(
            `marketing-engagement-${key}`,
            id,
            i.expectedVersion + 1,
            i.eventType,
            i.channel,
            i.reason,
            a.userId,
            now,
          ),
      ],
    );}
case "handoff":{const i=m.input,crm=m.crm;return executeCommand(
    write,
    a,
    key,
    op,
    i,
    id,
    i.expectedVersion + 1,
    result,
    [
      assertion(
        write,
        key,
        "candidate",
        "EXISTS(SELECT 1 FROM marketing_lead_candidates WHERE id=? AND version=? AND status='pending')",
        [id, i.expectedVersion],
      ),
      write
        .prepare(
          "UPDATE marketing_lead_candidates SET status='handed_off',crm_lead_id=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(crm.id, a.userId, now, id),
      write
        .prepare(
          "INSERT INTO marketing_engagement_events VALUES(?,? ,?,'crm_handoff','internal',?,?,?)",
        )
        .bind(
          `marketing-engagement-${key}`,
          id,
          i.expectedVersion + 1,
          i.reason,
          a.userId,
          now,
        ),
    ],
  );}
}}
async function translated<Result>(action:()=>Promise<Result>){try{return await action();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export function marketingWorkspace(a:WorkContext){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireMarketingPurchasedRead(db,a),view=await application(a,db).workspace();return {...view,...(purchase?{purchase,canWrite:hasPermission(a,'records.write'),costHistory:await marketingFinanceHistory(db,a)}: {})};});}
export function createMarketingCandidate(a:WorkContext,raw:unknown,key:string){return translated(()=>{const input=marketingCandidateSchema.parse(raw);return withMarketingPurchasedCommand(a,{key,operation:'marketing.candidate.create',target:{kind:'candidate',id:`marketing-candidate-${key}`},purpose:'new-work',sources:[{kind:'asset',id:input.formId}]},db=>application(a,db).createCandidate(input,key));});}
export function changeMarketing(a:WorkContext,kind:"asset"|"candidate",id:string,raw:unknown,key:string){return translated(()=>{const input=marketingActionSchema.parse(raw);return kind==='candidate'&&input.action==='handoff'?crmMarketingCandidateTransaction(a,id,input,key,(db,now)=>application(a,db,now).change(kind,id,input,key,repository(db,a,false))):withMarketingPurchasedCommand(a,{key,operation:kind==='asset'?`marketing.asset.${input.action}`:'marketing.candidate.engagement',target:{kind,id},existing:true,purpose:input.action==='activate'?'new-work':'settle-existing'},db=>application(a,db).change(kind,id,input,key));});}
export function marketingDetail(a:WorkContext,id:string){return translated(async()=>{const db=(await holdingResources(a.holdingId)).write,purchase=await requireMarketingPurchasedRead(db,a,{kind:'candidate',id}),view=await application(a,db).detail(id),handoff=purchase?await marketingHandoffControls(db,a):undefined;return {...view,...(purchase?{purchase,canWrite:hasPermission(a,'records.write'),canHandoff:!!handoff?.sales&&hasPermission(a,'records.write')&&view.candidate.created_by!==a.userId,handoffPurchase:handoff?.purchase}: {})};});}
export function marketingExecutiveEvidenceRow(db:SqlDatabase,a:WorkContext,metric:MarketingExecutiveEvidenceMetric,id:string,range?:{from:number;toExclusive:number}){return translated(async()=>{await requireMarketingPurchasedRead(db,a);return application(a,db).evidence(metric,id,range);});}
export function marketingWork(a:WorkContext){return translated(async()=>{if(!hasModuleAccess(a.modules,'marketing')||!hasPermission(a,'records.read'))return [];const db=(await holdingResources(a.holdingId)).write,purchase=await marketingSourceAccess(db,a);if(purchase&&!purchase.history)return [];const rows=await application(a,db).work();return filterMarketingPurchasedWork(db,a,purchase&&!purchase.settleExisting?rows.map(row=>({...row,canAct:false,queue:'waiting' as const})):rows);});}
