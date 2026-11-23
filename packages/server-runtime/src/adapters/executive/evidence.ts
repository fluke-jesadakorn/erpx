import "server-only";
import {headers} from "next/headers";
import {nativeBenefitsEvidenceRequest,nativeHoldingConfiguration} from "@erpx/native-sdk/benefits";
import { type Filters, type MetricId } from "@erpx/feature-shared/lib/executive/canvas";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { getSupplierBill } from "@erpx/server-procurement/public/supplier-bill-matching";
import { nativeQualityHoldDetail } from "../../../../feature-quality/src/public/server";
import {nativeFieldEvidence} from "@erpx/server-field_service/public/native-executive-evidence";
import { productionDetail } from "@erpx/server-manufacturing/public/production";
import { maintenanceDowntimeEvidenceRows } from "@erpx/server-maintenance/public/asset-maintenance";
import {getSupportIncident} from '@erpx/server-support/public/support-incident-read';
import { canReadOrganizationChanges } from "@erpx/server-organization/public/organization-work";
import { loadPms } from "../../../../../packages/server-pms/src/public/pms";
import {getOvertime} from '@erpx/server-people_payroll/public/overtime-read';
import { readAgentEvidence, searchAgentEvidence } from "@erpx/server-runtime/public/executive/agent-evidence";
import { readCrmEvidence, searchCrmEvidence } from "@erpx/server-runtime/public/executive/crm-evidence";
import { readBrandEvidence, searchBrandEvidence } from "@erpx/server-runtime/public/executive/brand-evidence";
import { readLearningEvidence, searchLearningEvidence } from "@erpx/server-runtime/public/executive/learning-evidence";
import { readTalentEvidence, searchTalentEvidence } from "@erpx/server-runtime/public/executive/talent-evidence";
import { readPosEvidence, searchPosEvidence } from "@erpx/server-runtime/public/executive/pos-evidence";
import { readMarketingEvidence, searchMarketingEvidence } from "@erpx/server-runtime/public/executive/marketing-evidence";
import { readPlanningEvidence, searchPlanningEvidence } from "@erpx/server-runtime/public/executive/planning-evidence";
import { executiveMainEvidenceSources as runtimeSources } from '@erpx/server-runtime/adapters/executive/main-evidence-sources';
import { executiveMainEvidenceSources as legalSources } from '@erpx/server-legal/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as financeSources } from '@erpx/server-finance/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as people_payrollSources } from '@erpx/server-people_payroll/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as pmsSources } from '@erpx/server-pms/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as procurementSources } from '@erpx/server-procurement/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as commerceSources } from '@erpx/server-commerce/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as qualitySources } from '@erpx/server-quality/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as manufacturingSources } from '@erpx/server-manufacturing/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as customer_360Sources } from '@erpx/server-customer_360/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as collaborationSources } from '@erpx/server-collaboration/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as supportSources } from '@erpx/server-support/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as customer_authoritySources } from '@erpx/server-customer_authority/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as organizationSources } from '@erpx/server-organization/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as identity_accessSources } from '@erpx/server-identity_access/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as billingSources } from '@erpx/server-billing/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as inventorySources } from '@erpx/server-inventory/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as projectsSources } from '@erpx/server-projects/public/executive-main-evidence-sources';
import { executiveMainEvidenceSources as operationsSources } from '@erpx/server-operations/public/executive-main-evidence-sources';
import { createExecutiveEvidenceApplication, type ExecutiveEvidenceRepository } from '@erpx/server-runtime/application/executive/evidence';
import { BusinessRuleError } from '@erpx/server-shared/index';
import type { FinanceMetricId } from '@erpx/server-runtime/domain/executive/evidence';
const nativeBenefitMetrics=["active-benefit-elections","published-benefit-plans"] as const;
const nativeBenefitFields=["id","status","version","updated_at","effective_from","effective_to"];
const nativeBenefitProvider="benefits-current-records";
type NativeBenefitMetric=typeof nativeBenefitMetrics[number];
type NativeBenefitSearchPayload={provider:string;metric:NativeBenefitMetric;fields:string[];items:{id:string;label:string;summary:string;status:string;version:number;href:string}[];hasMore:boolean;nextCursor:string|null;queryHash:string};
type NativeBenefitReadPayload={provider:string;metric:NativeBenefitMetric;fields:string[];record:{id:string;status:string;version:number;updated_at:number;effective_from:string;effective_to:string|null;href:string};queryHash:string};
const nativeBenefitItem=(value:unknown):value is {id:string;label:string;summary:string;status:string;version:number;href:string}=>object(value)&&text(value.id)&&text(value.label)&&text(value.summary)&&text(value.status)&&integer(value.version)&&value.version>0&&text(value.href);
const nativeBenefitRecord=(value:unknown):value is {id:string;status:string;version:number;updated_at:number;effective_from:string;effective_to:string|null;href:string}=>object(value)&&text(value.id)&&text(value.status)&&integer(value.version)&&value.version>0&&integer(value.updated_at)&&text(value.effective_from)&&(value.effective_to===null||text(value.effective_to))&&text(value.href);
function object(value:unknown):value is Record<string,unknown>{return !!value&&typeof value==="object"&&!Array.isArray(value);}
function text(value:unknown):value is string{return typeof value==="string"&&value.length>0;}
function integer(value:unknown):value is number{return typeof value==="number"&&Number.isSafeInteger(value);}
function sameSet(left:readonly string[],right:unknown){return Array.isArray(right)&&right.every(text)&&left.length===right.length&&new Set(left).size===new Set(right).size&&left.every(value=>right.includes(value));}
function nativeBenefitsFailure(code='NATIVE_BENEFITS_EVIDENCE_UNAVAILABLE',status=503):never{throw new ApiError(status,code,'The native Benefits evidence is unavailable.');}

async function nativeBenefitEvidence(actor:WorkContext,action:'search'|'open',input:unknown):Promise<NativeBenefitSearchPayload|NativeBenefitReadPayload>{
 const raw=process.env.ERPX_NATIVE_BENEFITS_CONFIG;if(!raw)nativeBenefitsFailure('NATIVE_BENEFITS_EVIDENCE_CONFIGURATION_REQUIRED');
 const profile=process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost'?'contributor':'public';
 let config:ReturnType<typeof nativeHoldingConfiguration>;try{config=nativeHoldingConfiguration(raw,profile);}catch{nativeBenefitsFailure('NATIVE_BENEFITS_EVIDENCE_CONFIGURATION_INVALID');}
 const incoming=await headers();
 const target=config.holdings.find(target=>new URL(target.origin).host===incoming.get('host')&&target.holdingId===actor.holdingId&&target.holdingSlug===actor.holdingSlug);
 if(!target||!actor.authority||!actor.iam)nativeBenefitsFailure('WORK_CONTEXT_CHANGED',409);
 const origin=incoming.get('origin');if(origin!==null&&origin!==target.origin)nativeBenefitsFailure('ORIGIN_FORBIDDEN',403);
 const h=new Headers({host:new URL(target.origin).host,accept:'application/json','content-type':'application/json','cache-control':'no-cache','x-erpx-context-company':actor.companyId,'x-erpx-context-assignment':actor.assignmentId,'x-erpx-context-revision':String(actor.authority.revision),'x-erpx-context-assignment-version':String(actor.authority.assignmentVersion)});
 const cookie=incoming.get('cookie');if(cookie){if(Buffer.byteLength(cookie)>8192)nativeBenefitsFailure();h.set('cookie',cookie);}if(origin)h.set('origin',origin);
 if(actor.scope.kind!=='holding'&&actor.scope.branchSlug)h.set('x-erpx-workspace-branch',actor.scope.branchSlug);
 if(actor.scope.kind==='department')h.set('x-erpx-workspace-department',actor.scope.departmentSlug);
 if(!object(input)||typeof input.metric!=='string'||!nativeBenefitMetrics.includes(input.metric as typeof nativeBenefitMetrics[number])||!object(input.filters))nativeBenefitsFailure('EVIDENCE_INPUT_INVALID',422);
 const filters=input.filters;
 const scope=[actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId];
 const value=action==='search'?{query:input.query??'',cursor:input.cursor??null}:{id:input.recordId};
 const queryHash=await sha256(JSON.stringify({provider:nativeBenefitProvider,metric:input.metric,filters:{from:filters.from,to:filters.to,currency:filters.currency},scope,value}));
 const body=JSON.stringify({...input,queryHash});
 let reply:Awaited<ReturnType<typeof nativeBenefitsEvidenceRequest>>;try{reply=await nativeBenefitsEvidenceRequest({baseUrl:config.privateBaseUrl,slug:target.holdingSlug,headers:h,timeoutMs:config.timeoutMs,action,body,production:config.production});}catch{nativeBenefitsFailure();}
 const b=reply.body;if(reply.status!==200){if(object(b)&&object(b.error)&&text(b.error.code))nativeBenefitsFailure(b.error.code,[401,403,404,409,422].includes(reply.status)?reply.status:503);nativeBenefitsFailure();}
 if(!object(b)||!object(b.context)||!object(b.data))nativeBenefitsFailure('NATIVE_BENEFITS_EVIDENCE_RESPONSE_INVALID',502);
 const c=b.context,expected={holdingId:actor.holdingId,userId:actor.userId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:actor.scope.kind==='holding'?null:actor.scope.branchSlug??null,departmentSlug:actor.scope.kind==='department'?actor.scope.departmentSlug:null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope};
 if(Object.entries(expected).some(([key,value])=>c[key]!==value)||!sameSet(actor.modules,c.modules)||!sameSet(actor.iam.permissions,c.permissions)||!integer(c.expiresAt)||c.expiresAt<=Date.now())nativeBenefitsFailure('WORK_CONTEXT_CHANGED',409);
 const d=b.data;
 if(d.provider!==nativeBenefitProvider||d.metric!==input.metric||!sameSet(nativeBenefitFields,d.fields)||d.queryHash!==queryHash)nativeBenefitsFailure('NATIVE_BENEFITS_EVIDENCE_RESPONSE_INVALID',502);
 if(action==='search'){
  if(!Array.isArray(d.items)||d.items.length>50||!d.items.every(nativeBenefitItem)||new Set(d.items.map(item=>item.id)).size!==d.items.length||typeof d.hasMore!=='boolean'||!(d.nextCursor===null||text(d.nextCursor)&&d.nextCursor.length<=2000)||d.hasMore!==(d.nextCursor!==null)||d.hasMore&&d.items.length===0)nativeBenefitsFailure('NATIVE_BENEFITS_EVIDENCE_RESPONSE_INVALID',502);
  return {provider:d.provider,metric:d.metric as NativeBenefitMetric,fields:[...nativeBenefitFields],items:d.items.map(({id,label,summary,status,version,href})=>({id,label,summary,status,version,href})),hasMore:d.hasMore,nextCursor:d.nextCursor,queryHash:d.queryHash};
 }
 const recordValue=d.record;
 if(!nativeBenefitRecord(recordValue)||!object(input)||recordValue.id!==input.recordId)nativeBenefitsFailure('NATIVE_BENEFITS_EVIDENCE_RESPONSE_INVALID',502);
 return {provider:d.provider,metric:d.metric as NativeBenefitMetric,fields:[...nativeBenefitFields],record:{id:recordValue.id,status:recordValue.status,version:recordValue.version,updated_at:recordValue.updated_at,effective_from:recordValue.effective_from,effective_to:recordValue.effective_to,href:recordValue.href},queryHash:d.queryHash};
}
async function searchNativeBenefitEvidence(actor:WorkContext,raw:unknown):Promise<NativeBenefitSearchPayload>{
 return await nativeBenefitEvidence(actor,'search',raw) as NativeBenefitSearchPayload;
}
async function readNativeBenefitEvidence(actor:WorkContext,metric:string,id:string,filters:Filters):Promise<NativeBenefitReadPayload>{
 return await nativeBenefitEvidence(actor,'open',{metric,filters,recordId:id}) as NativeBenefitReadPayload;
}

function application(db: SqlDatabase) { const repository: ExecutiveEvidenceRepository = { ...runtimeSources(db), ...legalSources(db), ...financeSources(db), ...people_payrollSources(db), ...pmsSources(db), ...procurementSources(db), ...commerceSources(db), ...qualitySources(db), ...manufacturingSources(db), ...customer_360Sources(db), ...collaborationSources(db), ...supportSources(db), ...customer_authoritySources(db), ...organizationSources(db), ...identity_accessSources(db), ...billingSources(db), ...inventorySources(db), ...projectsSources(db), ...operationsSources(db) }; return createExecutiveEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, currentAuthority: (...args) => fromNative(() => currentAuthority(db, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(db, ...args)), assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(db, ...args)), getSupplierBill: (...args) => fromNative(() => getSupplierBill(...args, db)), getQualityHold: (actor: WorkContext, id: string) => fromNative(async () => { const h = (await nativeQualityHoldDetail(actor, id)).hold; return { id: h.id, product_name: h.productName, warehouse_name: h.warehouseName, reason: h.reason, status: h.status, due_date: h.dueDate, created_at: h.createdAt, updated_at: h.updatedAt, version: h.version, released_by: h.releasedBy, released_at: h.releasedAt, release_reason: h.releaseReason }; }), productionDetail: (...args) => fromNative(() => productionDetail(...args, db)), getSupportIncident: (...args) => fromNative(() => getSupportIncident(...args, db)), canReadOrganizationChanges: (...args) => fromNative(() => canReadOrganizationChanges(db, ...args)), loadPms: (...args) => fromNative(() => loadPms(db, ...args)), getOvertime: (...args) => fromNative(() => getOvertime(...args, db)), maintenanceDowntimeEvidenceRows: (...args) => fromNative(() => maintenanceDowntimeEvidenceRows(...args, db)), searchAgentEvidence: (...args) => fromNative(() => searchAgentEvidence(db, ...args)), readAgentEvidence: (...args) => fromNative(() => readAgentEvidence(db, ...args)), searchCrmEvidence: (...args) => fromNative(() => searchCrmEvidence(db, ...args)), readCrmEvidence: (...args) => fromNative(() => readCrmEvidence(db, ...args)), searchBrandEvidence: (...args) => fromNative(() => searchBrandEvidence(db, ...args)), readBrandEvidence: (...args) => fromNative(() => readBrandEvidence(db, ...args)), searchLearningEvidence: (...args) => fromNative(() => searchLearningEvidence(db, ...args)), readLearningEvidence: (...args) => fromNative(() => readLearningEvidence(db, ...args)), searchTalentEvidence: (...args) => fromNative(() => searchTalentEvidence(db, ...args)), readTalentEvidence: (...args) => fromNative(() => readTalentEvidence(db, ...args)), searchPosEvidence: (...args) => fromNative(() => searchPosEvidence(db, ...args)), readPosEvidence: (...args) => fromNative(() => readPosEvidence(db, ...args)), searchMarketingEvidence: (...args) => fromNative(() => searchMarketingEvidence(db, ...args)), readMarketingEvidence: (...args) => fromNative(() => readMarketingEvidence(db, ...args)), searchBenefitEvidence: (...args) => fromNative(() => searchNativeBenefitEvidence(...args)), readBenefitEvidence: (...args) => fromNative(() => readNativeBenefitEvidence(...args)), searchPlanningEvidence: (...args) => fromNative(() => searchPlanningEvidence(db, ...args)), readPlanningEvidence: (...args) => fromNative(() => readPlanningEvidence(db, ...args)) }); }
async function fromNative<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function searchFinanceEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchFinanceEvidence(actor, raw)); }
export async function readFinanceEvidence(db: SqlDatabase, actor: WorkContext, metric: FinanceMetricId, recordId: string, filters: Filters) { return invoke(() => application(db).readFinanceEvidence(actor, metric, recordId, filters)); }
export async function searchExecutiveEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return raw&&typeof raw==='object'&&'metric' in raw&&raw.metric==='field-visit-completion'?nativeFieldEvidence(actor,'search',raw):invoke(() => application(db).searchExecutiveEvidence(actor, raw)); }
export async function readExecutiveEvidence(db: SqlDatabase, actor: WorkContext, metric: MetricId, recordId: string, filters: Filters) { return metric==='field-visit-completion'?nativeFieldEvidence(actor,'open',{metric,filters,recordId}):invoke(() => application(db).readExecutiveEvidence(actor, metric, recordId, filters)); }
export type { EvidenceSearchInput } from '@erpx/server-runtime/domain/executive/evidence';
