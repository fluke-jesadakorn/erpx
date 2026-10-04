import "server-only";
import {z} from "zod";
import type {WorkContext} from "./authorization";
import type {SqlDatabase,SqlStatement} from "./runtime/types";
import {hasPermission} from "@/lib/permissions";
import {hasModuleAccess} from "@/data/module-access";
import {holdingResources} from "./runtime";
import {requirePermission} from "./iam-permissions";
import {requireBrandManagement,readStorefrontAppBrand} from "./branding";
import {assertion,executeCommand,replayCommand} from "./commands";
import {ApiError} from "./http";
import {samePerson} from "@/lib/iam";
const reason=z.string().trim().min(3).max(4000),version=z.number().int().positive();
export const brandIdentitySchema=z.object({identity:z.object({shortName:z.string().trim().min(1).max(40),tagline:z.string().trim().max(160).default("")}).strict(),visual:z.object({accentColor:z.string().regex(/^#[a-fA-F0-9]{6}$/).transform(value=>value.toLowerCase()),logoPublicationId:z.string().min(1).nullable()}).strict()}).strict();
export type BrandIdentity=z.infer<typeof brandIdentitySchema>;
type Draft={id:string;company_id:string;based_revision:number;identity_json:string;status:string;version:number;created_by:string;updated_by:string;updated_at:number};
type Published={id:string;company_id:string;revision:number;identity_json:string;draft_id:string;draft_version:number;created_by:string;published_by:string;published_at:number};
type Lock={category:"identity"|"visual";locked:boolean;approved_snapshot_json:string;version:number;reason:string;updated_by:string;updated_at:number};
type Event={id:string;action:string;record_id:string;version:number;reason:string;actor_id:string;created_at:number;details_json:string};
export async function publishedBrandIdentity(db:SqlDatabase,companyId:string){
 // Existing profiles remain readable during an additive migration upgrade.
 if(!await db.prepare("SELECT to_regclass('brand_identity_active') relation").first("relation"))return null;
 const row=await db.prepare("SELECT v.*,a.version active_version FROM brand_identity_active a JOIN brand_identity_versions v ON v.id=a.identity_version_id AND v.company_id=a.company_id WHERE a.company_id=?").bind(companyId).first<Published&{active_version:number}>();
 return row?{...row,identity:brandIdentitySchema.parse(JSON.parse(row.identity_json))}:null;
}
async function authority(db:SqlDatabase,a:WorkContext,write=false){
 requirePermission(a,write?"organization.manage":"records.read");if(!hasModuleAccess(a.modules,"brand"))throw new ApiError(403,"MODULE_FORBIDDEN","Brand identity is unavailable to the selected role.");
 if(write)await requireBrandManagement(db,a,"company_storefront_brand",a.companyId);
 const current=await db.prepare("SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=? AND iam_assignment_active(g.id,?) AND iam_principal_active(g.user_id,g.company_id,?) AND iam_role_allows(g.role,g.company_id,?)").bind(a.assignmentId,a.userId,a.role,a.companyId,Date.now(),Date.now(),write?"organization.manage":"records.read").first();
 if(!current)throw new ApiError(403,"BRAND_AUTHORITY_CHANGED","Your current Brand authority is unavailable.");
}
function adminGuard(db:SqlDatabase,a:WorkContext,key:string,owner=false){return assertion(db,key,"brand-authority","EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=? AND g.scope_kind='holding' AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(g.role,g.company_id,?))",[a.assignmentId,a.userId,a.role,a.companyId,owner?"company.owner":"organization.manage"]);}
async function lockState(db:SqlDatabase,a:WorkContext){return (await db.prepare("SELECT * FROM brand_category_locks WHERE company_id=? ORDER BY category").bind(a.companyId).all<Lock>()).results;}
function checkLocks(locks:Lock[],input:BrandIdentity){for(const lock of locks)if(lock.locked&&JSON.stringify(input[lock.category])!==JSON.stringify(JSON.parse(lock.approved_snapshot_json)))throw new ApiError(409,"BRAND_CATEGORY_LOCKED",`The approved ${lock.category} category is locked. An owner must review and unlock it before a change.`);}
function lockGuard(db:SqlDatabase,a:WorkContext,key:string,locks:Lock[]){return assertion(db,key,"brand-lock-state","COALESCE((SELECT sum(version) FROM brand_category_locks WHERE company_id=?),0)=?",[a.companyId,locks.reduce((sum,lock)=>sum+Number(lock.version),0)]);}
function companyLock(db:SqlDatabase,a:WorkContext){return db.prepare("SELECT id FROM organization_nodes WHERE id=? AND kind='company' AND status='active' FOR UPDATE").bind(a.companyId);}
function mediaGuards(db:SqlDatabase,a:WorkContext,key:string,input:BrandIdentity):SqlStatement[]{return input.visual.logoPublicationId?[db.prepare("SELECT id FROM attachment_publications WHERE id=? FOR SHARE").bind(input.visual.logoPublicationId),assertion(db,key,"brand-approved-media","EXISTS(SELECT 1 FROM attachment_publications WHERE id=? AND company_id=? AND target_type='company_storefront_brand' AND target_id=? AND state='approved')",[input.visual.logoPublicationId,a.companyId,a.companyId])]:[];}
function event(db:SqlDatabase,a:WorkContext,key:string,id:string,action:string,v:number,why:string,details:unknown){return db.prepare("INSERT INTO brand_identity_events VALUES(?,?,?,?,?,?,?,?,?)").bind(`brand-event-${key}`,a.companyId,id,action,v,a.userId,why,JSON.stringify(details),Date.now());}
export async function brandIdentityWorkspace(a:WorkContext){
 const {write}=await holdingResources(a.holdingId);await authority(write,a);const canManage=hasPermission(a,"organization.manage")&&!a.branchId&&!a.departmentId,[drafts,versions,locks,history,active,legacy,media]=await Promise.all([
  write.prepare("SELECT * FROM brand_identity_drafts WHERE company_id=? AND (?::boolean OR created_by=?) ORDER BY updated_at DESC").bind(a.companyId,canManage,a.userId).all<Draft>(),
  write.prepare("SELECT * FROM brand_identity_versions WHERE company_id=? ORDER BY revision DESC").bind(a.companyId).all<Published>(),lockState(write,a),
  write.prepare("SELECT * FROM brand_identity_events WHERE company_id=? ORDER BY created_at DESC,id LIMIT 200").bind(a.companyId).all<Event>(),publishedBrandIdentity(write,a.companyId),readStorefrontAppBrand(write,a),
  write.prepare("SELECT id,alt_text name FROM attachment_publications WHERE company_id=? AND target_type='company_storefront_brand' AND target_id=? AND state='approved' ORDER BY approved_at DESC").bind(a.companyId,a.companyId).all<{id:string;name:string}>(),
 ]);
 const initial=active?.identity??{identity:{shortName:legacy.shortName,tagline:""},visual:{accentColor:legacy.accentColor,logoPublicationId:null}};
 return {drafts:drafts.results,versions:versions.results,locks,history:canManage?history.results:[],active,initial,media:media.results,actorId:a.userId,canManage,canLock:hasPermission(a,"company.owner")&&!a.branchId&&!a.departmentId};
}
export async function saveBrandIdentityDraft(a:WorkContext,raw:unknown,key:string){
 const input=z.object({id:z.string().min(1).optional(),expectedVersion:version.optional(),expectedPublishedVersion:z.number().int().nonnegative(),identity:brandIdentitySchema,reason}).parse(raw),{write}=await holdingResources(a.holdingId);await authority(write,a,true);
 const recordId=input.id??`brand-draft-${key}`,op=input.id?"brand.identity.save":"brand.identity.draft",prior=await replayCommand(write,a,key,op,recordId,input);if(prior)return prior;
 const active=await publishedBrandIdentity(write,a.companyId),locks=await lockState(write,a);checkLocks(locks,input.identity);
 if(input.expectedPublishedVersion!==(active?.active_version??0))throw new ApiError(409,"BRAND_PUBLICATION_CHANGED","The published identity changed. Review it before saving.");
 const row=input.id?await write.prepare("SELECT * FROM brand_identity_drafts WHERE id=? AND company_id=?").bind(input.id,a.companyId).first<Draft>():null;
 if(input.id&&(!row||row.status!=="draft"))throw new ApiError(404,"BRAND_DRAFT_UNAVAILABLE","The editable draft is unavailable.");
 const v=(input.expectedVersion??0)+1,now=Date.now(),steps=[companyLock(write,a),adminGuard(write,a,key),lockGuard(write,a,key,locks),...mediaGuards(write,a,key,input.identity),assertion(write,key,"published-version","COALESCE((SELECT version FROM brand_identity_active WHERE company_id=?),0)=?",[a.companyId,input.expectedPublishedVersion])];
 if(row){if(!input.expectedVersion)throw new ApiError(422,"EXPECTED_VERSION_REQUIRED","Use the current draft version.");steps.push(assertion(write,key,"draft-version","EXISTS(SELECT 1 FROM brand_identity_drafts WHERE id=? AND company_id=? AND version=? AND status='draft')",[recordId,a.companyId,input.expectedVersion]),write.prepare("UPDATE brand_identity_drafts SET identity_json=?,based_revision=?,version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(JSON.stringify(input.identity),active?.revision??0,a.userId,now,recordId));}
 else steps.push(write.prepare("INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,created_by,updated_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)").bind(recordId,a.companyId,active?.revision??0,JSON.stringify(input.identity),a.userId,a.userId,now,now));
 steps.push(event(write,a,key,recordId,input.id?"save":"draft",v,input.reason,{identity:input.identity,basedRevision:active?.revision??0}));
 return executeCommand(write,a,key,op,input,recordId,v,{id:recordId,status:"draft",version:v},steps);
}
export async function publishBrandIdentity(a:WorkContext,draftId:string,raw:unknown,key:string){
 const input=z.object({expectedVersion:version,expectedPublishedVersion:z.number().int().nonnegative(),reason}).parse(raw),{write}=await holdingResources(a.holdingId);await authority(write,a,true);const op="brand.identity.publish",prior=await replayCommand(write,a,key,op,draftId,input);if(prior)return prior;
 const row=await write.prepare("SELECT * FROM brand_identity_drafts WHERE id=? AND company_id=? AND status='draft'").bind(draftId,a.companyId).first<Draft>();if(!row)throw new ApiError(404,"BRAND_DRAFT_UNAVAILABLE","The editable draft is unavailable.");
 if(samePerson(a,row.created_by)||samePerson(a,row.updated_by))throw new ApiError(403,"BRAND_INDEPENDENT_PUBLICATION","Another authorized person must review and publish this draft.");
 const identity=brandIdentitySchema.parse(JSON.parse(row.identity_json)),active=await publishedBrandIdentity(write,a.companyId),locks=await lockState(write,a);checkLocks(locks,identity);
 if(row.based_revision!==(active?.revision??0))throw new ApiError(409,"BRAND_DRAFT_REBASE_REQUIRED","The published identity advanced. Save a reviewed draft against the current identity first.");
 const now=Date.now(),revision=Number(await write.prepare("SELECT COALESCE(max(revision),0)+1 revision FROM brand_identity_versions WHERE company_id=?").bind(a.companyId).first("revision")),publishedId=`brand-version-${key}`,publicationVersion=input.expectedPublishedVersion+1;
 return executeCommand(write,a,key,op,input,draftId,input.expectedVersion+1,{id:draftId,status:"published",version:input.expectedVersion+1,publishedId,revision,publishedVersion:publicationVersion},[
  companyLock(write,a),adminGuard(write,a,key),lockGuard(write,a,key,locks),...mediaGuards(write,a,key,identity),
  assertion(write,key,"published-version","COALESCE((SELECT version FROM brand_identity_active WHERE company_id=?),0)=?",[a.companyId,input.expectedPublishedVersion]),
  assertion(write,key,"independent-draft","EXISTS(SELECT 1 FROM brand_identity_drafts WHERE id=? AND company_id=? AND version=? AND status='draft' AND NOT iam_same_person(created_by,?,company_id) AND NOT iam_same_person(updated_by,?,company_id))",[draftId,a.companyId,input.expectedVersion,a.userId,a.userId]),
  write.prepare("INSERT INTO brand_identity_versions VALUES(?,?,?,?,?,?,?,?,?)").bind(publishedId,a.companyId,revision,row.identity_json,draftId,input.expectedVersion,row.created_by,a.userId,now),
  write.prepare("UPDATE brand_identity_drafts SET status='published',version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(a.userId,now,draftId),
  ...activate(write,a,publishedId,publicationVersion,identity,now),event(write,a,key,publishedId,"publish",publicationVersion,input.reason,{draftId,draftVersion:input.expectedVersion,revision,identity}),
 ]);
}
function activate(db:SqlDatabase,a:WorkContext,publishedId:string,v:number,identity:BrandIdentity,now:number){return [
 db.prepare("INSERT INTO brand_identity_active VALUES(?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET identity_version_id=excluded.identity_version_id,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(a.companyId,publishedId,v,a.userId,now),
 db.prepare("INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_publication_id,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET short_name=excluded.short_name,accent_color=excluded.accent_color,logo_publication_id=excluded.logo_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(a.companyId,identity.identity.shortName,identity.visual.accentColor,identity.visual.logoPublicationId,a.userId,now),
 ];}
export async function rollbackBrandIdentity(a:WorkContext,raw:unknown,key:string){
 const input=z.object({publishedId:z.string().min(1),expectedPublishedVersion:version,reason}).parse(raw),{write}=await holdingResources(a.holdingId);await authority(write,a,true);const op="brand.identity.rollback",prior=await replayCommand(write,a,key,op,a.companyId,input);if(prior)return prior;
 const row=await write.prepare("SELECT * FROM brand_identity_versions WHERE id=? AND company_id=?").bind(input.publishedId,a.companyId).first<Published>();if(!row)throw new ApiError(404,"BRAND_VERSION_UNAVAILABLE","Choose a published identity from this Company.");
 const identity=brandIdentitySchema.parse(JSON.parse(row.identity_json)),locks=await lockState(write,a);checkLocks(locks,identity);const v=input.expectedPublishedVersion+1;
 return executeCommand(write,a,key,op,input,a.companyId,v,{id:input.publishedId,revision:row.revision,publishedVersion:v},[companyLock(write,a),adminGuard(write,a,key),lockGuard(write,a,key,locks),...mediaGuards(write,a,key,identity),assertion(write,key,"published-version","EXISTS(SELECT 1 FROM brand_identity_active WHERE company_id=? AND version=?)",[a.companyId,input.expectedPublishedVersion]),...activate(write,a,row.id,v,identity,Date.now()),event(write,a,key,row.id,"rollback",v,input.reason,{revision:row.revision,identity})]);
}
export async function changeBrandCategoryLock(a:WorkContext,raw:unknown,key:string){
 const input=z.object({category:z.enum(["identity","visual"]),locked:z.boolean(),expectedVersion:z.number().int().nonnegative(),expectedPublishedVersion:version,reason}).parse(raw),{write}=await holdingResources(a.holdingId);await authority(write,a,true);requirePermission(a,"company.owner");
 const id=`${a.companyId}:${input.category}`,op=input.locked?"brand.category.lock":"brand.category.unlock",prior=await replayCommand(write,a,key,op,id,input);if(prior)return prior;const active=await publishedBrandIdentity(write,a.companyId);if(!active)throw new ApiError(409,"BRAND_PUBLISHED_IDENTITY_REQUIRED","Publish an independently reviewed identity before locking categories.");
 const v=input.expectedVersion+1;return executeCommand(write,a,key,op,input,id,v,{category:input.category,locked:input.locked,version:v},[companyLock(write,a),adminGuard(write,a,key,true),assertion(write,key,"lock-version","COALESCE((SELECT version FROM brand_category_locks WHERE company_id=? AND category=?),0)=?",[a.companyId,input.category,input.expectedVersion]),assertion(write,key,"published-version","EXISTS(SELECT 1 FROM brand_identity_active WHERE company_id=? AND version=?)",[a.companyId,input.expectedPublishedVersion]),
  write.prepare("INSERT INTO brand_category_locks VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(company_id,category) DO UPDATE SET locked=excluded.locked,approved_snapshot_json=excluded.approved_snapshot_json,version=excluded.version,updated_by=excluded.updated_by,reason=excluded.reason,updated_at=excluded.updated_at").bind(a.companyId,input.category,input.locked,JSON.stringify(active.identity[input.category]),v,a.userId,input.reason,Date.now()),event(write,a,key,id,input.locked?"lock":"unlock",v,input.reason,{category:input.category,publishedId:active.id,locked:input.locked}),
 ]);
}
export async function cancelBrandDraft(a:WorkContext,draftId:string,raw:unknown,key:string){
 const input=z.object({expectedVersion:version,reason}).parse(raw),{write}=await holdingResources(a.holdingId);await authority(write,a,true);const op="brand.identity.cancel",prior=await replayCommand(write,a,key,op,draftId,input);if(prior)return prior;
 return executeCommand(write,a,key,op,input,draftId,input.expectedVersion+1,{id:draftId,status:"cancelled",version:input.expectedVersion+1},[companyLock(write,a),adminGuard(write,a,key),assertion(write,key,"draft-version","EXISTS(SELECT 1 FROM brand_identity_drafts WHERE id=? AND company_id=? AND status='draft' AND version=?)",[draftId,a.companyId,input.expectedVersion]),write.prepare("UPDATE brand_identity_drafts SET status='cancelled',version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(a.userId,Date.now(),draftId),event(write,a,key,draftId,"cancel",input.expectedVersion+1,input.reason,{})]);
}
