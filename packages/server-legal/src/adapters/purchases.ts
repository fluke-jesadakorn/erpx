import 'server-only';
import {randomUUID} from 'node:crypto';
import {featureSpecs} from '@erpx/feature-legal/lib/legal/features';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {assignmentReadSql} from '@erpx/server-identity_access/public/record-source-authority';
import {iamMutationGuards} from '@erpx/server-identity_access/public/iam-store';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {legalPackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability,companyPurchaseReadLocks,purchaseCommandBoundary,purchaseCommandFailure} from '@erpx/server-commercial/public/purchase-command-boundary';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
import type {CapabilityPurpose} from '@erpx/server-commercial/public/rules/packages';
import type {LegalKind} from '@erpx/server-legal/domain/model';
import type {LegalEvidenceSource} from '@erpx/server-legal/adapters/evidence-attachment-read';
import {requireQueuedCommandAuthority,requireQueuedAccountAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {legalKindCapability,type LegalPurchaseCapability,type LegalPurchaseControls} from '@erpx/server-legal/domain/purchases';
export type {LegalPurchaseCapability,LegalPurchaseControls} from '@erpx/server-legal/domain/purchases';
const capabilities:LegalPurchaseCapability[]=['legal.contracts','legal.documents','legal.matters','legal.obligations','legal.access'];
const kinds:Record<LegalPurchaseCapability,string[]>={'legal.contracts':['contract','template'],'legal.documents':[],'legal.matters':['request','matter','counsel','ip'],'legal.obligations':['obligation','compliance','license','corporate','privacy','rule'],'legal.access':[]};
export function legalItemPurchasePredicate(alias:string,purpose:CapabilityPurpose='history',clock?:string){const sampled=clock??'legal_purchase_clock.now_ms',predicate=`(${capabilities.filter(code=>kinds[code].length).map(code=>`(${alias}.kind IN (${kinds[code].map(kind=>`'${kind}'`).join(',')}) AND ${purchasedCapabilityPredicate(`${alias}.company_id`,code,purpose,sampled)})`).join(' OR ')})`;return clock?predicate:`EXISTS(WITH legal_purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM legal_purchase_clock WHERE ${predicate})`;}
export async function requireCurrentLegalCommandAuthority(database:SqlDatabase,actor:WorkContext,key:string,permission:string){
 const row=await database.prepare(`SELECT ${assignmentReadSql('legal_current_scope',permission)} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) legal_current_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first('allowed');
 if(!row)throw new ApiError(403,'LEGAL_ACCESS_CHANGED','Choose a current Legal assignment.');
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(key)]);
}
export async function legalPurchaseControls(database:SqlDatabase,actor:WorkContext):Promise<LegalPurchaseControls|undefined>{const mode=await legalPackageEnforcementMode();if(mode==='disabled')return;await requireCurrentLegalCommandAuthority(database,actor,`legal-read:${randomUUID()}`,'legal.read');const access=await readPurchasedAccesses(database,mode,actor.companyId,capabilities);return Object.fromEntries(capabilities.map(code=>[code,{newWork:access[code].newWork,settleExisting:access[code].settleExisting,history:access[code].history,revision:access[code].revision}]));}
/** Preserve the original Company-manager preparation path for rules/templates;
 * this does not release operational Legal workflows or broaden rollout. */
export function legalReleasePredicate(actor:WorkContext,alias='i'){const scheduled=scheduledFeaturePredicate(`${alias}.company_id`,'legal');return !actor.branchId&&!actor.departmentId&&hasPermission(actor,'legal.manage')?`(${scheduled} OR ${alias}.kind IN ('rule','template'))`:scheduled;}
export async function legalReadScope(database:SqlDatabase,actor:WorkContext,alias='i'){const access=await legalPurchaseControls(database,actor);return {where:access&&await legalPackageEnforcementMode()==='enforced'?`${legalItemPurchasePredicate(alias)} AND ${legalReleasePredicate(actor,alias)}`:'TRUE',purchases:access};}
export async function requireLegalItemPurchasedRead(database:SqlDatabase,actor:WorkContext,item:{id:string;kind:LegalKind},additional:LegalPurchaseCapability[]=[]){const access=await legalPurchaseControls(database,actor);if(access&&[legalKindCapability(item.kind),...additional].some(code=>!access[code]?.history))throw new ApiError(403,'PACKAGE_REQUIRED','This Legal source requires its purchased capability or retained history.');if(access&&await legalPackageEnforcementMode()==='enforced'&&!await database.prepare(`SELECT 1 FROM legal_items i WHERE i.id=? AND i.company_id=? AND ${legalReleasePredicate(actor)}`).bind(item.id,actor.companyId).first())throw new ApiError(403,'FEATURE_COMING_SOON','Legal is unavailable for this Company.');return access;}
const fileCommands=new WeakMap<SqlDatabase,{id:string;userId:string;purpose:CapabilityPurpose}>();
export function legalOwnedAttachmentPurpose(database:SqlDatabase,actor:WorkContext,id:string){const command=fileCommands.get(database);return command?.id===id&&command.userId===actor.userId?command.purpose:null;}
type ConfigurationSource={table:"legal_settings"|"legal_intake_forms"|"legal_retention_policies";id:string};
async function configurationSource(database:SqlDatabase,actor:WorkContext,target:ConfigurationSource){return database.prepare(`SELECT to_jsonb(configuration)::text image FROM ${target.table} configuration WHERE ${target.table==='legal_settings'?'company_id':'id'}=? AND company_id=?`).bind(target.id,actor.companyId).first<string>("image");}
type Unit={companyId:string;checks:(()=>Promise<void>)[];attachmentReadPreparing?:boolean};const units=new WeakMap<SqlDatabase,Unit>();
async function inLegalUnit<Result>(owner:SqlDatabase,companyId:string,run:(database:SqlDatabase)=>Promise<Result>,beforeSnapshot:((database:SqlDatabase)=>Promise<void>)|undefined,mapError:boolean){
 if(units.get(owner)?.companyId===companyId){if(beforeSnapshot)throw new ApiError(503,"LEGAL_FRESH_OWNER_UNIT_REQUIRED","This Legal operation requires its own fresh transaction.");return run(owner);}
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'LEGAL_TRANSACTION_REQUIRED','Legal requires its complete owned Holding transaction.');
 try{return await owner.transaction(async database=>{const unit={companyId,checks:[] as (()=>Promise<void>)[]};units.set(database,unit);try{const purchaseLocks=companyPurchaseReadLocks(database,companyId);await database.batch([purchaseLocks[0],database.prepare('LOCK TABLE legal_items,legal_members,legal_documents,legal_shares IN SHARE ROW EXCLUSIVE MODE')]);if(beforeSnapshot)await beforeSnapshot(database);await database.batch(purchaseLocks.slice(1));await database.prepare('SELECT pg_advisory_xact_lock(hashtextextended(?,0))').bind(`legal:${companyId}`).run();const result=await run(database);await flushFinanceDeferredEffects(database);for(const check of unit.checks)await check();return result;}finally{units.delete(database);}});}catch(error){if(!mapError)throw error;throw purchaseCommandFailure(error)??financialConstraintError(error)??error;}
}
export function inLegalOwnerUnit<Result>(owner:SqlDatabase,companyId:string,run:(database:SqlDatabase)=>Promise<Result>,beforeSnapshot?:(database:SqlDatabase)=>Promise<void>){return inLegalUnit(owner,companyId,run,beforeSnapshot,true);}
/** Original mode-off writer operations own one outer unit and retain native
 * constraint errors. This never adopts an arbitrary caller SQL transaction. */
export function inLegacyLegalOperationsUnit<Result>(owner:SqlDatabase,companyId:string,run:(database:SqlDatabase)=>Promise<Result>,beforeSnapshot?:(database:SqlDatabase)=>Promise<void>){return inLegalUnit(owner,companyId,run,beforeSnapshot,false);}
export async function legalSourceWitness(database:SqlDatabase,actor:Pick<WorkContext,"companyId">,id:string){const item=await source(database,actor,id);if(!item)return null;const table=({request:'legal_requests',matter:'legal_matters',contract:'legal_contracts',obligation:'legal_obligations',compliance:'legal_compliance',license:'legal_licenses',corporate:'legal_corporate',ip:'legal_ip_assets',privacy:'legal_privacy',counsel:'legal_counsel',template:'legal_templates',rule:'legal_rules'} as const)[item.kind as LegalKind];return database.prepare(`SELECT jsonb_build_object('item',to_jsonb(i),'data',(SELECT data FROM ${table} WHERE item_id=i.id),'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM legal_members m WHERE m.item_id=i.id),'documents',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM legal_documents d WHERE d.item_id=i.id),'shares',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM legal_shares s WHERE s.item_id=i.id),'executed',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM legal_executed_copies e WHERE e.item_id=i.id),'approvals',(SELECT jsonb_agg(to_jsonb(ap) ORDER BY ap.cycle) FROM legal_approvals ap WHERE ap.item_id=i.id),'route',(SELECT to_jsonb(ar) FROM legal_approval_routes ar WHERE ar.item_id=i.id),'authorities',(SELECT jsonb_agg(to_jsonb(au) ORDER BY au.id) FROM legal_authorities au WHERE au.item_id=i.id),'retention',(SELECT to_jsonb(ra)||jsonb_build_object('policy',(SELECT to_jsonb(rp) FROM legal_retention_policies rp WHERE rp.id=ra.policy_id)) FROM legal_retention_assignments ra WHERE ra.item_id=i.id),'destructions',(SELECT jsonb_agg(to_jsonb(dx) ORDER BY dx.id) FROM legal_destructions dx WHERE dx.item_id=i.id),'features',(SELECT jsonb_agg(feature.image ORDER BY feature.image::text) FROM (${Object.values(featureSpecs).map(spec=>`SELECT to_jsonb(f) image FROM ${spec.table} f WHERE f.item_id=i.id`).join(' UNION ALL ')}) feature))::text image FROM legal_items i WHERE i.id=? AND i.company_id=?`).bind(id,actor.companyId).first<string>('image');}
async function source(database:SqlDatabase,actor:Pick<WorkContext,"companyId">,id:string){return database.prepare('SELECT to_jsonb(i) image FROM legal_items i WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{image:Record<string,unknown>}>().then(row=>row?typeof row.image==='string'?JSON.parse(row.image) as Record<string,unknown>:row.image:null);}
const identity=(row:Record<string,unknown>)=>JSON.stringify(['id','company_id','branch_id','department_id','kind','created_by','created_at'].map(key=>[key,row[key]]));
export async function withLegalPurchasedCommand<Result>(owner:SqlDatabase,actor:WorkContext,command:{key:string;operation:string;id?:string;createKind?:LegalKind;purpose:CapabilityPurpose;permission:string;additionalPermissions?:string[];additional?:LegalPurchaseCapability[];configuration?:boolean;ownerSource?:ConfigurationSource},run:(database:SqlDatabase)=>Promise<Result>){
 const mode=await legalPackageEnforcementMode();if(mode==='disabled')return run(owner);
 return inLegalOwnerUnit(owner,actor.companyId,async database=>{
  const prior=await database.prepare("SELECT operation FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(command.key,actor.userId,actor.companyId).first<{operation:string}>(),replay=prior?.operation===command.operation,purpose=replay?'history':command.purpose,before=command.id?await source(database,actor,command.id):null;
  if(command.id&&!command.createKind&&!before)throw new ApiError(404,'LEGAL_NOT_FOUND','Legal source not found in this Company.');
  const kind=(before?.kind??command.createKind) as LegalKind|undefined,scopes=[...new Set<LegalPurchaseCapability>([...(kind?[legalKindCapability(kind)]:[]),...(command.configuration?['legal.access' as const]:[]),...(command.additional??[])])].map(capability=>({companyId:actor.companyId,capability,purpose}));
  const key=`legal-purchase:${randomUUID()}`,boundaries=scopes.map((scope,index)=>({key:`${key}:${index}`,boundary:purchaseCommandBoundary(database,mode,`${key}:${index}`,scope)!}));for(const scope of scopes)await checkPurchasedCapability(database,mode,scope);for(const permission of [command.permission,...(command.additionalPermissions??[])])await requireCurrentLegalCommandAuthority(database,actor,key,permission);await database.batch(boundaries.flatMap(item=>item.boundary.before));
  const previous=fileCommands.get(database);if(command.id)fileCommands.set(database,{id:command.id,userId:actor.userId,purpose});let result:Result;try{result=await run(database);}finally{if(previous)fileCommands.set(database,previous);else fileCommands.delete(database);}
  const expected=command.id?await source(database,actor,command.id):null,expectedWitness=command.id?await legalSourceWitness(database,actor,command.id):null;if(command.id&&(!expected||before&&identity(before)!==identity(expected)))throw new ApiError(409,'LEGAL_SOURCE_CHANGED','The original Legal source changed during its command.');
  const expectedConfiguration=command.ownerSource?await configurationSource(database,actor,command.ownerSource):null;if(command.ownerSource&&!expectedConfiguration)throw new ApiError(409,"LEGAL_CONFIGURATION_CHANGED","The original Legal configuration source changed.");
  units.get(database)!.checks.push(async()=>{if(command.ownerSource&&expectedConfiguration!==await configurationSource(database,actor,command.ownerSource))throw new ApiError(409,"LEGAL_CONFIGURATION_CHANGED","The completed Legal configuration changed during deferred effects.");if(command.id&&(JSON.stringify(expected)!==JSON.stringify(await source(database,actor,command.id))||expectedWitness!==await legalSourceWitness(database,actor,command.id)))throw new ApiError(409,'LEGAL_SOURCE_CHANGED','The completed Legal source changed during deferred effects.');if(command.id&&mode==='enforced'&&!await database.prepare(`SELECT 1 FROM legal_items i WHERE i.id=? AND i.company_id=? AND ${legalReleasePredicate(actor)}`).bind(command.id,actor.companyId).first())throw new ApiError(403,'FEATURE_COMING_SOON','Legal is unavailable for this Company.');for(const permission of [command.permission,...(command.additionalPermissions??[])])await requireCurrentLegalCommandAuthority(database,actor,key,permission);const observations=await database.batch(boundaries.flatMap(item=>[...item.boundary.after,database.prepare('DELETE FROM command_assertions WHERE command_id=?').bind(item.key)]));boundaries[0]?.boundary.observe?.(observations);});return result;
 });
}

export function deferLegalOwnerCheck(database:SqlDatabase,companyId:string,check:()=>Promise<void>){const unit=units.get(database);if(!unit||unit.companyId!==companyId)throw new ApiError(503,'LEGAL_TRANSACTION_REQUIRED','The complete Legal owner transaction is required.');unit.checks.push(check);}

/** Closed preparation path: the original fresh Legal constructor invokes the
 * Task reader before its first data statement. A caller's arbitrary snapshot
 * or the general Legal callback cannot mint this preparation phase. */
export function inLegalAttachmentPreservationUnit<Result>(owner:SqlDatabase,actor:WorkContext,run:(database:SqlDatabase)=>Promise<Result>,source?:LegalEvidenceSource){
 return inLegalOwnerUnit(owner,actor.companyId,run,async database=>{
  const unit=units.get(database)!;unit.attachmentReadPreparing=true;
  try{const canonical=await import('@erpx/server-legal/adapters/canonical-attachment-read');await canonical.prepareLegalCanonicalAttachmentParentRead(database,actor);if(source){const evidence=await import('@erpx/server-legal/adapters/evidence-attachment-read');await evidence.prepareLegalEvidenceAttachmentParentRead(database,actor,source);}const reader=await import('@erpx/server-operations/public/task-attachment-read');await reader.prepareWorkTaskAttachmentParentRead(database,actor);await canonical.prepareLegalCanonicalAttachmentSharedSources(database);}
  finally{unit.attachmentReadPreparing=false;}
 });
}
/** Canonical document reads prepare their actual Legal sources before the
 * original owner constructor's first Company advisory/data statement. */
export function inLegalCanonicalDocumentReadUnit<Result>(owner:SqlDatabase,actor:WorkContext,permission:'legal.read'|'legal.export',run:(database:SqlDatabase)=>Promise<Result>){
 return inLegalOwnerUnit(owner,actor.companyId,async database=>{deferLegalAttachmentReadCheck(database,actor.companyId,async()=>{const canonical=await import('@erpx/server-legal/adapters/canonical-attachment-read');const denial=canonical.legalCanonicalDenialReadAuthority(database,actor);if(denial==='current')await requireQueuedAccountAuthority(database,actor);else if(!denial)await requireQueuedCommandAuthority(database,actor,permission);});return run(database);},async database=>{
  const unit=units.get(database)!;unit.attachmentReadPreparing=true;
  try{const canonical=await import('@erpx/server-legal/adapters/canonical-attachment-read');await canonical.prepareLegalCanonicalAttachmentParentRead(database,actor);await canonical.prepareLegalCanonicalAttachmentSharedSources(database);}
  finally{unit.attachmentReadPreparing=false;}
 });
}
export function requireLegalAttachmentReadPreparation(database:SqlDatabase,companyId:string){
 const unit=units.get(database);if(unit?.companyId!==companyId||!unit.attachmentReadPreparing)throw new ApiError(503,'LEGAL_ATTACHMENT_READ_PREPARATION_REQUIRED','Task evidence requires the original fresh Legal preservation constructor.');
}
export function deferLegalAttachmentReadCheck(database:SqlDatabase,companyId:string,check:()=>Promise<void>){
 const unit=units.get(database);if(unit?.companyId!==companyId||unit.attachmentReadPreparing)throw new ApiError(503,'LEGAL_ATTACHMENT_READ_PARENT_REQUIRED','The original complete Legal preservation transaction is required.');
 unit.checks.push(check);
}
