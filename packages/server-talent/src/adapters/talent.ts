import {withTalentPurchasedCommand,withTalentPurchasedRead,talentPurchaseControls} from "@erpx/server-talent/adapters/purchases";
import {nativeTalentReadAll,nativeTalentReadFirst} from "./native-talent-read";
import {SensitiveAttachmentTalentFacts} from "@erpx/server-talent/adapters/attachments-sensitive-facts";
import {talentActionSchema} from "@erpx/server-talent/domain/talent";
import {learningTalentSnapshotSources as identityaccessSnapshotSources} from '@erpx/server-identity_access/public/learning-talent-snapshot-sources';
import {learningTalentSnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/learning-talent-snapshot-sources';
const identityaccessSources=identityaccessSnapshotSources();
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import {selectProjection} from "./talent-read-tail-r37-sources";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import { ApiError } from "@erpx/server-runtime/public/http";
import { decimalToMinor } from "@erpx/feature-finance/lib/finance-money";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { createTalentWorkspace, createTalentOffer as createTalentOfferApplication, createChangeTalentRecord, createTalentDetail, createTalentWork, type TalentCommandPorts, type TalentCommand, type TalentMutation } from "@erpx/server-talent/application/talent";
import type { TalentKind } from "@erpx/server-talent/domain/talent";
function access(a: WorkContext, write = false) {
  requirePermission(a, write ? "records.write" : "records.read");
  if (!hasModuleAccess(a.modules, "talent"))
    throw new ApiError(
      403,
      "MODULE_FORBIDDEN",
      "Your current role cannot access Talent.",
    );
  if (!hasPermission(a, "records.review"))
    throw new ApiError(
      403,
      "TALENT_PRIVACY_FORBIDDEN",
      "Candidate data requires Talent reviewer access.",
    );
}
const eligibleOwner = `EXISTS(${selectProjection('1')} ${identityaccessSources.talent_adapters_talent_assignments_x_from} WHERE x.company_id=a.company_id AND x.user_id=a.owner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=r.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=r.department_id)))`;
/** Owner-local application-projection head family. The projected columns and
 * trailing predicate are parameterized; the owner-eligibility witness and the
 * FROM/JOIN skeleton are shared, so both call sites stay byte-identical to the
 * former inline copies. */
const applicationProjection = (columns:string,tail:string) => `${selectProjection(`a.*,r.job_title,${columns},CASE WHEN ${eligibleOwner} THEN 1 ELSE 0 END owner_eligible`)} FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=a.candidate_id WHERE ${tail}`;
const ownedApps=applicationProjection('c.legal_name candidate_name','a.company_id=? AND ');

type DatabaseSource=SqlDatabase|(()=>Promise<SqlDatabase>);
const databaseSource=(source:DatabaseSource)=>typeof source==="function"?source:async()=>source;
function lazyDatabase(a:WorkContext){let resource:Promise<SqlDatabase>|null=null;return()=>resource??=holdingResources(a.holdingId).then(resources=>resources.write);}
function commandPorts(database:()=>Promise<SqlDatabase>):TalentCommandPorts{return{authorize:access,error:violation=>new ApiError(violation.status,violation.code,violation.message),now:Date.now,today:()=>new Date().toISOString().slice(0,10),replay:async(a,key,op,id,input)=>replayCommand(await database(),a,key,op,id,input),commit:async(command:TalentCommand,mutation:TalentMutation)=>executeCommand(await database(),command.actor,command.key,command.operation,command.input,command.id,command.version,command.result,mutation.native as SqlStatement[])};}
function recordSource(kind:TalentKind){const table=kind === "requisition"
        ? "talent_requisitions"
        : kind === "application"
          ? "talent_applications"
          : kind === "proposal"
            ? "talent_proposals"
            : "talent_offers",join=kind==="requisition"?"":kind==="application"?"JOIN talent_requisitions r ON r.id=t.requisition_id":"JOIN talent_applications ap ON ap.id=t.application_id JOIN talent_requisitions r ON r.id=ap.requisition_id",companyRef=kind==="proposal"?"ap":"t",placementRef=kind==="requisition"?"t":"r";return {table,join,companyRef,placementRef};}
async function recruiterReferences(write:SqlDatabase,a:WorkContext){return (await write
        .prepare(
          `${selectProjection(`DISTINCT x.user_id id,COALESCE(p.legal_name,x.user_id) name ${identityaccessSources.talent_adapters_talent_assignments_x_from} LEFT ${peoplepayrollSources.talent_adapters_talent_hr_people_p_join} ON p.company_id=x.company_id AND p.user_id=x.user_id AND p.merged_into_id IS NULL WHERE x.company_id=? AND iam_assignment_active(x.id,?) AND iam_principal_active(x.user_id,x.company_id,?) ORDER BY 2`)}`,
        )
        .bind(a.companyId, Date.now(), Date.now())
        .all<Row>()).results;}
export function nativeTalentWorkspace(source:DatabaseSource){const database=databaseSource(source);return createTalentWorkspace({authorize:access,
async requisitions(a){return nativeTalentReadAll<Row>('talent_rd_workspace_requisitions',[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId],await database());},
async candidates(a){return nativeTalentReadAll<Row>('talent_rd_workspace_candidates',[a.companyId],await database());},
async applications(a){const write=await database();return (await write
        .prepare(
          `${ownedApps}(?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()).results;},
async people(a){return recruiterReferences(await database(),a);}
});}
export async function talentWorkspace(a:WorkContext){return withTalentPurchasedRead(a,["recruit.positions","recruit.candidates","recruit.applications","recruit.offers"],async db=>({...await nativeTalentWorkspace(db??lazyDatabase(a))(a),canWrite:hasPermission(a,"records.write"),purchases:await talentPurchaseControls(a,db)}));}
// Wave 3: offer writes execute only in the native axum Talent lifecycle route
// (talent_lifecycle). No TypeScript SQL writer remains for this capability;
// callers must use the native command client and fail closed when unavailable.
type NativeTalentWriteResult={id:string;[key:string]:unknown;};
export function nativeTalentOffer(_source:DatabaseSource){
  return (_a:WorkContext,_raw:unknown,_key:string):Promise<NativeTalentWriteResult>=>Promise.reject(new ApiError(503,'NATIVE_TALENT_UNAVAILABLE','The native Talent offer command is unavailable.'));
}
export async function createTalentOffer(a:WorkContext,raw:unknown,key:string){return withTalentPurchasedCommand(a,{key,operation:"talent.offer.create",capability:"recruit.offers",purpose:"new-work",target:{kind:"offer",id:`talent-offer-${key}`},creating:true,permissions:["records.write","records.review"]},db=>nativeTalentOffer(db??lazyDatabase(a))(a,raw,key));}
// Wave 3: record changes execute only in the native axum Talent lifecycle
// route. No TypeScript SQL writer remains for this capability.
export function nativeTalentChanges(_source:DatabaseSource){
  return (_a:WorkContext,_kind:TalentKind,_id:string,_raw:unknown,_key:string):Promise<NativeTalentWriteResult>=>Promise.reject(new ApiError(503,'NATIVE_TALENT_UNAVAILABLE','The native Talent record command is unavailable.'));
}
export async function changeTalentRecord(a:WorkContext,kind:TalentKind,id:string,raw:unknown,key:string){access(a,true);const input=talentActionSchema.parse(raw);return withTalentPurchasedCommand(a,{key,operation:`talent.${kind}.${input.action}`,capability:kind==="requisition"?"recruit.positions":kind==="offer"?"recruit.offers":"recruit.applications",purpose:["withdraw","decline","reject","reject-proposal","close","cancel","reassign","accept-offer"].includes(input.action)?"settle-existing":"new-work",target:{kind,id},permissions:["records.write","records.review"]},db=>nativeTalentChanges(db??lazyDatabase(a))(a,kind,id,raw,key));}
export function nativeTalentDetail(source:DatabaseSource){const database=databaseSource(source);return createTalentDetail({authorize:access,error:violation=>new ApiError(violation.status,violation.code,violation.message),
async application(a,id){const write=await database();return await write
      .prepare(
        applicationProjection('r.branch_id,r.department_id,c.legal_name candidate_name,c.email candidate_email,c.retention_until,c.version candidate_version','a.id=? AND a.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)'),
      )
      .bind(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId)
      .first<Row>();},
async history(id){return nativeTalentReadAll<Row>('talent_rd_application_events',[id],await database());},
async proposal(id){return nativeTalentReadFirst<Row>('talent_rd_proposals',[id],await database());},
async offer(id){return nativeTalentReadFirst<Row>('talent_rd_offers',[id],await database());},
async handoff(offerId){return nativeTalentReadFirst<Row>('talent_rd_hire_handoffs',[offerId],await database());},
});}
export async function talentDetail(a:WorkContext,id:string){return withTalentPurchasedRead(a,["recruit.candidates","recruit.applications","recruit.offers"],async db=>{
 const database=db??await lazyDatabase(a)(),detail=await nativeTalentDetail(database)(a,id),offerDocuments=detail.canManageDocuments&&detail.offer?await SensitiveAttachmentTalentFacts(database).readTalentOffers1({targetId:String(detail.offer.id),companyId:a.companyId}):null;
 return {...detail,offerDocumentsEditable:offerDocuments?.pending_hire_delivery===0,owners:await recruiterReferences(database,a),canWrite:hasPermission(a,"records.write"),purchases:await talentPurchaseControls(a,database)};
});}
async function talentWorkOwned(a:WorkContext,provided?:SqlDatabase):Promise<import("@erpx/feature-work_projection/lib/work-item").WorkItem[]>{const database=provided?async()=>provided:lazyDatabase(a);return createTalentWork({authorize:access,async rows(a){const write=await database();return (
      await write
        .prepare(
          `${ownedApps}a.status NOT IN ('accepted','rejected','withdrawn') AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) ORDER BY a.updated_at DESC`,
        )
        .bind(
          a.companyId,
          a.branchId,
          a.branchId,
          a.departmentId,
          a.departmentId,
        )
        .all<Row>()
    ).results;}})(a);}

export async function talentWork(a:WorkContext):Promise<import("@erpx/feature-work_projection/lib/work-item").WorkItem[]>{try{access(a);const purchase=(await talentPurchaseControls(a))?.["recruit.applications"];if(purchase&&!purchase.history)return [];const items=await withTalentPurchasedRead(a,["recruit.applications","recruit.candidates"],db=>talentWorkOwned(a,db));return purchase?items.map(item=>({...item,canAct:item.canAct&&(item.status==="offered"?purchase.settleExisting:purchase.newWork),queue:!purchase.newWork&&item.status!=="offered"?"waiting" as const:item.queue})):items;}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return [];throw error;}}
