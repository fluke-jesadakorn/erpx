import { commercialOwnerProjectionSources as filesProjectionSources } from "../../files/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/commercial-owner-projection-sources";
import { readBrandIdentityWorkspace, readPublishedBrandIdentity, type BrandIdentityEvent } from "../application/identity-read";
import { createBrandIdentityApplication, type BrandIdentityPorts, type BrandMutation } from "../application/identity";
import { type BrandIdentity, type BrandDraft as Draft, type BrandPublished as Published, type BrandCategoryLock as Lock } from "../domain/identity";
import { BrandRuleError } from "../domain/policy";
import "server-only";
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "@/server/modules/runtime/public/runtime/types";
import {hasPermission} from "@/lib/permissions";
import {hasModuleAccess} from "@/data/module-access";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {requireBrandManagement,readStorefrontAppBrand} from "@/server/branding";
import {assertion,executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {samePerson} from "@/lib/iam";

export function publishedBrandIdentity(db:SqlDatabase,companyId:string){
 return readPublishedBrandIdentity({
  async available(){return !!await db.prepare("SELECT to_regclass('brand_identity_active') relation").first("relation");},
  active:()=>db.prepare("SELECT v.*,a.version active_version FROM brand_identity_active a JOIN brand_identity_versions v ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id=?").bind(companyId).first<Published&{active_version:number}>(),
 });
}
async function authority(db:SqlDatabase,a:WorkContext,write=false){
 requirePermission(a,write?"organization.manage":"records.read");if(!hasModuleAccess(a.modules,"brand"))throw new ApiError(403,"MODULE_FORBIDDEN","Brand identity is unavailable to the selected role.");
 if(write)await requireBrandManagement(db,a,"company_storefront_brand",a.companyId);
 const current=await db.prepare(`SELECT 1 FROM (${identity_accessProjectionSources.assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=? AND iam_assignment_active(g.id,?) AND iam_principal_active(g.user_id,g.company_id,?) AND iam_role_allows(g.role,g.company_id,?)`).bind(a.assignmentId,a.userId,a.role,a.companyId,Date.now(),Date.now(),write?"organization.manage":"records.read").first();
 if(!current)throw new ApiError(403,"BRAND_AUTHORITY_CHANGED","Your current Brand authority is unavailable.");
}
function adminGuard(db:SqlDatabase,a:WorkContext,key:string,owner=false){return assertion(db,key,"brand-authority",`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=? AND g.scope_kind='holding' AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(g.role,g.company_id,?))`,[a.assignmentId,a.userId,a.role,a.companyId,owner?"company.owner":"organization.manage"]);}
async function lockState(db:SqlDatabase,a:WorkContext){return (await db.prepare("SELECT * FROM brand_category_locks WHERE company_id=? ORDER BY category").bind(a.companyId).all<Lock>()).results;}
function lockGuard(db:SqlDatabase,a:WorkContext,key:string,locks:Lock[]){return assertion(db,key,"brand-lock-state","COALESCE((SELECT sum(version) FROM brand_category_locks WHERE company_id=?),0)=?",[a.companyId,locks.reduce((sum,lock)=>sum+Number(lock.version),0)]);}
function companyLock(db:SqlDatabase,a:WorkContext){return db.prepare(`SELECT id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=? AND kind='company' AND status='active' FOR UPDATE`).bind(a.companyId);}
function mediaGuards(db:SqlDatabase,a:WorkContext,key:string,input:BrandIdentity):SqlStatement[]{return input.visual.logoPublicationId?[db.prepare(`SELECT id FROM (${filesProjectionSources.attachment_publications}) attachment_publications WHERE id=? FOR SHARE`).bind(input.visual.logoPublicationId),assertion(db,key,"brand-approved-media",`EXISTS(SELECT 1 FROM (${filesProjectionSources.attachment_publications}) attachment_publications WHERE id=? AND company_id=? AND target_type='company_storefront_brand' AND target_id=? AND state='approved')`,[input.visual.logoPublicationId,a.companyId,a.companyId])]:[];}
function event(db:SqlDatabase,a:WorkContext,key:string,id:string,action:string,v:number,why:string,details:unknown){return db.prepare("INSERT INTO brand_identity_events VALUES(?,?,?,?,?,?,?,?,?)").bind(`brand-event-${key}`,a.companyId,id,action,v,a.userId,why,JSON.stringify(details),Date.now());}
export async function brandIdentityWorkspace(a:WorkContext){
 const {write}=await holdingResources(a.holdingId);
 return readBrandIdentityWorkspace({actorId:a.userId,canManage:hasPermission(a,"organization.manage")&&!a.branchId&&!a.departmentId,canLock:hasPermission(a,"company.owner")&&!a.branchId&&!a.departmentId,
 authorize:()=>authority(write,a),
 async drafts(canManage){return (await write.prepare("SELECT * FROM brand_identity_drafts WHERE company_id=? AND (?::boolean OR created_by=?) ORDER BY updated_at DESC").bind(a.companyId,canManage,a.userId).all<Draft>()).results;},
 async versions(){return (await write.prepare("SELECT * FROM brand_identity_versions WHERE company_id=? ORDER BY revision DESC").bind(a.companyId).all<Published>()).results;},locks:()=>lockState(write,a),
 async history(){return (await write.prepare("SELECT * FROM brand_identity_events WHERE company_id=? ORDER BY created_at DESC,id LIMIT 200").bind(a.companyId).all<BrandIdentityEvent>()).results;},
 active:()=>publishedBrandIdentity(write,a.companyId),legacy:()=>readStorefrontAppBrand(write,a),
 async media(){return (await write.prepare(`SELECT id,alt_text name FROM (${filesProjectionSources.attachment_publications}) attachment_publications WHERE company_id=? AND target_type='company_storefront_brand' AND target_id=? AND state='approved' ORDER BY approved_at DESC`).bind(a.companyId,a.companyId).all<{id:string;name:string}>()).results;},
 });
}

function activate(db:SqlDatabase,a:WorkContext,publishedId:string,v:number,identity:BrandIdentity,now:number){return [
 db.prepare("INSERT INTO brand_identity_active VALUES(?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET identity_version_id=excluded.identity_version_id,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(a.companyId,publishedId,v,a.userId,now),
 db.prepare("INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_publication_id,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET short_name=excluded.short_name,accent_color=excluded.accent_color,logo_publication_id=excluded.logo_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(a.companyId,identity.identity.shortName,identity.visual.accentColor,identity.visual.logoPublicationId,a.userId,now),
 ];}

async function applicationFor(a: WorkContext) {
 const {write}=await holdingResources(a.holdingId);
 const ports:BrandIdentityPorts={
  companyId:a.companyId,clock:{now:()=>Date.now()},isSamePerson:(id)=>samePerson(a,id),
  async authorize(writeAccess,owner){await authority(write,a,writeAccess);if(owner)requirePermission(a,"company.owner");},
  replay:(key,operation,id,input)=>replayCommand(write,a,key,operation,id,input),
  active:()=>publishedBrandIdentity(write,a.companyId),locks:()=>lockState(write,a),
  draft:(id)=>write.prepare("SELECT * FROM brand_identity_drafts WHERE id=? AND company_id=?").bind(id,a.companyId).first<Draft>(),
  published:(id)=>write.prepare("SELECT * FROM brand_identity_versions WHERE id=? AND company_id=?").bind(id,a.companyId).first<Published>(),
  async nextRevision(){return Number(await write.prepare("SELECT COALESCE(max(revision),0)+1 revision FROM brand_identity_versions WHERE company_id=?").bind(a.companyId).first("revision"));},
  commit:(mutation,result)=>commitBrandIdentity(write,a,mutation,result),
 };
 return createBrandIdentityApplication(ports);
}

async function commitBrandIdentity<Result>(write:SqlDatabase,a:WorkContext,m:BrandMutation,result:Result):Promise<Result>{
 const {key,recordId:id,version:v,now}=m;
 let steps:SqlStatement[];
 switch(m.action){
  case "save":{
   steps=[companyLock(write,a),adminGuard(write,a,key),lockGuard(write,a,key,m.locks),...mediaGuards(write,a,key,m.identity),assertion(write,key,"published-version","COALESCE((SELECT version FROM brand_identity_active WHERE company_id=?),0)=?",[a.companyId,m.expectedPublishedVersion])];
   if(m.existing)steps.push(assertion(write,key,"draft-version","EXISTS(SELECT 1 FROM brand_identity_drafts WHERE id=? AND company_id=? AND version=? AND status='draft')",[id,a.companyId,m.expectedVersion!]),write.prepare("UPDATE brand_identity_drafts SET identity_json=?,based_revision=?,version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(JSON.stringify(m.identity),m.active?.revision??0,a.userId,now,id));
   else steps.push(write.prepare("INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,created_by,updated_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)").bind(id,a.companyId,m.active?.revision??0,JSON.stringify(m.identity),a.userId,a.userId,now,now));
   steps.push(event(write,a,key,id,m.existing?"save":"draft",v,m.reason,{identity:m.identity,basedRevision:m.active?.revision??0}));
   break;
  }
  case "publish":{
   const publicationVersion=m.expectedPublishedVersion+1;
   steps=[companyLock(write,a),adminGuard(write,a,key),lockGuard(write,a,key,m.locks),...mediaGuards(write,a,key,m.identity),
    assertion(write,key,"published-version","COALESCE((SELECT version FROM brand_identity_active WHERE company_id=?),0)=?",[a.companyId,m.expectedPublishedVersion]),
    assertion(write,key,"independent-draft","EXISTS(SELECT 1 FROM brand_identity_drafts WHERE id=? AND company_id=? AND version=? AND status='draft' AND NOT iam_same_person(created_by,?,company_id) AND NOT iam_same_person(updated_by,?,company_id))",[id,a.companyId,m.expectedVersion,a.userId,a.userId]),
    write.prepare("INSERT INTO brand_identity_versions VALUES(?,?,?,?,?,?,?,?,?)").bind(m.publishedId,a.companyId,m.revision,m.draft.identity_json,id,m.expectedVersion,m.draft.created_by,a.userId,now),
    write.prepare("UPDATE brand_identity_drafts SET status='published',version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(a.userId,now,id),
    ...activate(write,a,m.publishedId,publicationVersion,m.identity,now),event(write,a,key,m.publishedId,"publish",publicationVersion,m.reason,{draftId:id,draftVersion:m.expectedVersion,revision:m.revision,identity:m.identity})];
   break;
  }
  case "rollback":
   steps=[companyLock(write,a),adminGuard(write,a,key),lockGuard(write,a,key,m.locks),...mediaGuards(write,a,key,m.identity),assertion(write,key,"published-version","EXISTS(SELECT 1 FROM brand_identity_active WHERE company_id=? AND version=?)",[a.companyId,m.expectedPublishedVersion]),...activate(write,a,m.published.id,v,m.identity,now),event(write,a,key,m.published.id,"rollback",v,m.reason,{revision:m.published.revision,identity:m.identity})];
   break;
  case "lock":
   steps=[companyLock(write,a),adminGuard(write,a,key,true),assertion(write,key,"lock-version","COALESCE((SELECT version FROM brand_category_locks WHERE company_id=? AND category=?),0)=?",[a.companyId,m.category,m.expectedVersion]),assertion(write,key,"published-version","EXISTS(SELECT 1 FROM brand_identity_active WHERE company_id=? AND version=?)",[a.companyId,m.expectedPublishedVersion]),
    write.prepare("INSERT INTO brand_category_locks VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(company_id,category) DO UPDATE SET locked=excluded.locked,approved_snapshot_json=excluded.approved_snapshot_json,version=excluded.version,updated_by=excluded.updated_by,reason=excluded.reason,updated_at=excluded.updated_at").bind(a.companyId,m.category,m.locked,JSON.stringify(m.active.identity[m.category]),v,a.userId,m.reason,now),event(write,a,key,id,m.locked?"lock":"unlock",v,m.reason,{category:m.category,publishedId:m.active.id,locked:m.locked})];
   break;
  case "cancel":
   steps=[companyLock(write,a),adminGuard(write,a,key),assertion(write,key,"draft-version","EXISTS(SELECT 1 FROM brand_identity_drafts WHERE id=? AND company_id=? AND version=? AND status='draft')",[id,a.companyId,m.expectedVersion]),write.prepare("UPDATE brand_identity_drafts SET status='cancelled',version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(a.userId,now,id),event(write,a,key,id,"cancel",v,m.reason,{})];
   break;
 }
 return executeCommand(write,a,key,m.operation,m.input,id,v,result,steps);
}

async function translateRule<Result>(action:()=>Promise<Result>):Promise<Result>{try{return await action();}catch(error){if(error instanceof BrandRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export async function saveBrandIdentityDraft(a:WorkContext,raw:unknown,key:string){return translateRule(async()=>(await applicationFor(a)).save(raw,key));}
export async function publishBrandIdentity(a:WorkContext,id:string,raw:unknown,key:string){return translateRule(async()=>(await applicationFor(a)).publish(id,raw,key));}
export async function rollbackBrandIdentity(a:WorkContext,raw:unknown,key:string){return translateRule(async()=>(await applicationFor(a)).rollback(raw,key));}
export async function changeBrandCategoryLock(a:WorkContext,raw:unknown,key:string){return translateRule(async()=>(await applicationFor(a)).changeLock(raw,key));}
export async function cancelBrandDraft(a:WorkContext,id:string,raw:unknown,key:string){return translateRule(async()=>(await applicationFor(a)).cancel(id,raw,key));}
