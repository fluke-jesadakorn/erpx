import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {legalPackageEnforcementMode,checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {legalKindCapability} from '@erpx/server-legal/domain/purchases';
import type {LegalKind} from '@erpx/server-legal/domain/model';
import {inLegalOwnerUnit} from '@erpx/server-legal/adapters/purchases';
import {visibility} from '@erpx/server-legal/adapters/legal/access';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/hospitality-rollout';
type Source={id:string;item_id:string;company_id:string;kind:LegalKind;item_version:number;requested_by:string;approved_by:string|null;image:string};
async function source(database:SqlDatabase,id:string){return database.prepare(`SELECT x.id,x.item_id,i.company_id,i.kind,i.version item_version,x.requested_by,x.approved_by,jsonb_build_object('request',x.id,'item',i.id,'company',i.company_id,'branch',i.branch_id,'department',i.department_id,'kind',i.kind,'classification',i.classification,'state',i.state,'version',i.version,'requester',x.requested_by,'approver',x.approved_by,'manifest',x.manifest,'policyId',x.policy_id,'policyVersion',x.policy_version,'policy',to_jsonb(p))::text image FROM legal_destructions x JOIN legal_items i ON i.id=x.item_id JOIN legal_retention_policies p ON p.id=x.policy_id AND p.company_id=i.company_id WHERE x.id=?`).bind(id).first<Source>();}
const hash=(image:string)=>createHash('sha256').update(image).digest('hex');
function queuedAuthority(actor:WorkContext){const {holdingId,holdingSlug,holdingName,companyId,branchId,departmentId,userId,assignmentId,role,authority,iam,name,email,scope,assignmentScope,inherited,modules}=actor;return {holdingId,holdingSlug,holdingName,companyId,branchId,departmentId,userId,assignmentId,role,authority,iam,name,email,scope,assignmentScope,inherited,modules,assignments:[],workspaces:[]};}
/** Called only after the original complete approve/retry receipt in its owned
 * transaction. Original approvals and populated legacy rows are never rewritten. */
export async function recordLegalRetentionExecution(database:SqlDatabase,actor:WorkContext,key:string,operation:string,raw:unknown){
 if(await legalPackageEnforcementMode()==='disabled'||!['retention.approve','retention.retry'].includes(operation))return;
 const input=raw as {requestId?:unknown},id=typeof input?.requestId==='string'?input.requestId:null,original=id?await source(database,id):null;
 if(!original||original.company_id!==actor.companyId||!original.approved_by)throw new ApiError(409,'LEGAL_RETENTION_SOURCE_CHANGED','The original approved destruction source is unavailable.');
 if(await database.prepare('SELECT iam_same_person(?,?,?) same').bind(actor.userId,original.requested_by,actor.companyId).first<boolean>('same'))throw new ApiError(403,'LEGAL_INDEPENDENT_REVIEW','An independent current manager must authorize retained destruction work.');
 if(operation==='retention.approve'&&original.approved_by!==actor.userId)throw new ApiError(409,'LEGAL_RETENTION_SOURCE_CHANGED','The original approval identity changed.');
 const bound=await requireQueuedCommandAuthority(database,actor,'legal.destroy');
 await requireSourceAccess(database,bound,original.item_id);
 await database.prepare('INSERT INTO legal_retention_execution_authorizations(command_id,destruction_id,item_id,company_id,kind,actor_id,assignment_id,authority_json,source_hash,item_version,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)').bind(key,original.id,original.item_id,actor.companyId,operation==='retention.approve'?'approve':'retry',actor.userId,actor.assignmentId,JSON.stringify(queuedAuthority(bound)),hash(original.image),original.item_version,Date.now()).run();
}
async function requireSourceAccess(database:SqlDatabase,actor:WorkContext,itemId:string){const visible=visibility(actor);if(!await database.prepare(`SELECT 1 FROM legal_items i JOIN legal_settings s ON s.company_id=i.company_id AND s.enabled=true WHERE i.id=? AND ${visible.sql} AND ${scheduledFeaturePredicate('i.company_id','legal')}`).bind(itemId,...visible.values).first())throw new ApiError(403,'LEGAL_RETENTION_ACCESS_CHANGED','The exact retained Legal source is unavailable to its original queued reviewer.');}
type Lease={id:string;companyId:string;authorizedForDeletion:boolean};const leases=new WeakMap<SqlDatabase,Lease>();
async function assertCurrent(database:SqlDatabase,id:string){
 const lease=leases.get(database);if(!lease||lease.id!==id)throw new ApiError(503,'LEGAL_RETENTION_EXECUTION_UNIT_REQUIRED','Retained destruction requires its complete source-leased owner unit.');
 const original=await source(database,id),authorization=await database.prepare('SELECT * FROM legal_retention_execution_authorizations WHERE destruction_id=? ORDER BY item_version DESC,created_at DESC,command_id DESC LIMIT 1').bind(id).first<{company_id:string;item_id:string;actor_id:string;assignment_id:string;authority_json:string;source_hash:string;item_version:number}>();
 if(!original||!authorization||original.company_id!==lease.companyId||authorization.company_id!==original.company_id||authorization.item_id!==original.item_id||authorization.item_version!==original.item_version||authorization.source_hash!==hash(original.image))throw new ApiError(409,'LEGAL_RETENTION_SOURCE_CHANGED','Review the exact retained destruction source through its existing retry workflow.');
 const actor=JSON.parse(authorization.authority_json) as WorkContext;if(actor.userId!==authorization.actor_id||actor.assignmentId!==authorization.assignment_id||actor.companyId!==original.company_id)throw new ApiError(409,'LEGAL_RETENTION_SOURCE_CHANGED','The original queued authority binding changed.');
 await requireQueuedCommandAuthority(database,actor,'legal.destroy');
 await requireSourceAccess(database,actor,original.item_id);
 if(await database.prepare("SELECT 1 FROM legal_hold_targets t JOIN legal_holds h ON h.id=t.hold_id WHERE t.item_id=? AND h.state<>'released'").bind(original.item_id).first())throw new ApiError(409,'LEGAL_HOLD','A current Legal hold prevents deletion.');
 if(await database.prepare('SELECT iam_same_person(?,?,?) same').bind(actor.userId,original.requested_by,actor.companyId).first<boolean>('same'))throw new ApiError(403,'LEGAL_INDEPENDENT_REVIEW','The retained destruction requires its independent authorized reviewer.');
 const mode=await legalPackageEnforcementMode();for(const capability of [legalKindCapability(original.kind),'legal.obligations'])await checkPurchasedCapability(database,mode,{companyId:original.company_id,capability,purpose:'history'});
}
/** These units preserve the existing durable journal before physical deletion.
 * A later database failure never claims to restore already deleted bytes. */
export async function withLegalRetentionExecutionUnit(owner:PostgresDatabase,companyId:string,id:string,run:(database:SqlDatabase)=>Promise<void>){
 if(await legalPackageEnforcementMode()==='disabled')return owner.transaction(run);
 return inLegalOwnerUnit(owner,companyId,async database=>{leases.set(database,{id,companyId,authorizedForDeletion:false});try{await assertCurrent(database,id);await run(database);await flushFinanceDeferredEffects(database);await assertCurrent(database,id);}finally{leases.delete(database);}},async database=>{await database.prepare('LOCK TABLE legal_settings,legal_retention_execution_authorizations,legal_destructions,legal_retention_assignments,legal_retention_policies,legal_holds,legal_hold_targets,legal_disposals,legal_file_preservations IN SHARE ROW EXCLUSIVE MODE').run();await lockQueuedCommandAuthority(database);});
}
export async function authorizeLegalRetentionDeletion(database:SqlDatabase,id:string){if(await legalPackageEnforcementMode()==='disabled')return;await flushFinanceDeferredEffects(database);await assertCurrent(database,id);leases.get(database)!.authorizedForDeletion=true;}
/** Both local and remote private stores call this named owner check. */
export async function requireLegalRetentionDeletion(database:SqlDatabase,id:string){if(await legalPackageEnforcementMode()==='disabled')return;if(!leases.get(database)?.authorizedForDeletion)throw new ApiError(503,'LEGAL_RETENTION_EXECUTION_UNIT_REQUIRED','Storage requires the original retained-deletion unit.');await assertCurrent(database,id);}
