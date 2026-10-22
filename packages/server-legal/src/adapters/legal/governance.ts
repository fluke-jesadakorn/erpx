import {withLegalPurchasedCommand,legalPurchaseControls,requireLegalItemPurchasedRead,inLegacyLegalOperationsUnit} from '@erpx/server-legal/adapters/purchases';
import {legalPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import "server-only";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {access,guards,eligible,personGuard,lock} from "@erpx/server-legal/adapters/legal/access";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import {createLegalGovernanceApplication} from "@erpx/server-legal/application/governance";
function application(db:SqlDatabase,a:WorkContext){const nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
return createLegalGovernanceApplication({actor:a,access:(id,p)=>access(db,a,id,p),eligible:(id,item,p)=>eligible(db,a,id,item,p),samePerson:id=>samePerson(a,id),require:permission=>requirePermission(a,permission),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
forms:(facts:{companyId:string;})=>db.prepare("SELECT * FROM legal_intake_forms WHERE company_id=? ORDER BY name").bind(facts.companyId).all(),
authorities:(facts:{id:string;})=>db.prepare("SELECT * FROM legal_authorities WHERE item_id=? ORDER BY id").bind(facts.id).all(),
settings:(facts:{companyId:string;})=>db.prepare("SELECT enabled,ai_enabled,reminder_days FROM legal_settings WHERE company_id=?").bind(facts.companyId).first(),
createForm:(facts:{key:string;companyId:string;name:string;fieldsJson:string;slaDays:number;actorId:string;})=>stage(db.prepare("INSERT INTO legal_intake_forms(id,company_id,name,fields,sla_days,created_by) VALUES(?,?,?,?::jsonb,?,?)").bind(facts.key,facts.companyId,facts.name,facts.fieldsJson,facts.slaDays,facts.actorId)),
form:(facts:{formId:string;companyId:string;})=>db.prepare("SELECT * FROM legal_intake_forms WHERE id=? AND company_id=?").bind(facts.formId,facts.companyId).first<{ created_by: string; state: string; version: number }>(),
approveForm:(facts:{actorId:string;formId:string;})=>stage(db.prepare("UPDATE legal_intake_forms SET state='approved',approved_by=?,version=version+1 WHERE id=?").bind(facts.actorId,facts.formId)),
approvedForm:(facts:{formId:string;companyId:string;})=>db.prepare("SELECT fields,sla_days FROM legal_intake_forms WHERE id=? AND company_id=? AND state='approved'").bind(facts.formId,facts.companyId).first<{ fields: string; sla_days: number }>(),
escalationMember:(facts:{itemId:string;escalationOwner:string;})=>db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?").bind(facts.itemId,facts.escalationOwner).first(),
applyIntake:(facts:{formId:string;answersJson:string;now:number;fact3:string;fact4:string | null;itemId:string;})=>stage(db.prepare("UPDATE legal_items SET intake_form_id=?,intake_answers=?::jsonb,sla_started_at=?,sla_due_at=floor(extract(epoch FROM ((clock_timestamp() AT TIME ZONE timezone + (? || ' days')::interval) AT TIME ZONE timezone))*1000)::bigint,escalation_owner=? WHERE id=?").bind(facts.formId,facts.answersJson,facts.now,facts.fact3,facts.fact4,facts.itemId)),
authorityDocument:(facts:{documentId:string;itemId:string;})=>db.prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=?").bind(facts.documentId,facts.itemId).first(),
signerAccess:(facts:{itemId:string;userId:string;itemId2:string;userId2:string;now:number;})=>db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? UNION ALL SELECT 1 FROM legal_shares WHERE item_id=? AND user_id=? AND revoked_at IS NULL AND expires_at>?").bind(facts.itemId,facts.userId,facts.itemId2,facts.userId2,facts.now).first(),
createAuthority:(facts:{key:string;itemId:string;companyId:string;userId:string;documentId:string;currency:string;limitMinor:number;validFrom:string;validUntil:string;actorId:string;})=>stage(db.prepare("INSERT INTO legal_authorities(id,item_id,company_id,user_id,document_id,currency,limit_minor,valid_from,valid_until,state,created_by) VALUES(?,?,?,?,?,?,?,?,?,'proposed',?)").bind(facts.key,facts.itemId,facts.companyId,facts.userId,facts.documentId,facts.currency,facts.limitMinor,facts.validFrom,facts.validUntil,facts.actorId)),
authority:(facts:{authorityId:string;itemId:string;})=>db.prepare("SELECT * FROM legal_authorities WHERE id=? AND item_id=?").bind(facts.authorityId,facts.itemId).first<{ created_by: string; state: string; version: number }>(),
decideAuthority:(facts:{fact0:"approved" | "revoked";fact1:string | null;authorityId:string;})=>stage(db.prepare("UPDATE legal_authorities SET state=?,approved_by=?,version=version+1 WHERE id=?").bind(facts.fact0,facts.fact1,facts.authorityId)),
advanceRecord:(facts:{now:number;itemId:string;})=>stage(db.prepare("UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?").bind(facts.now,facts.itemId)),
requireFormCurrent:(key,values)=>stage(assertion(db,key,"form-current","EXISTS(SELECT 1 FROM legal_intake_forms WHERE id=? AND version=? AND state='draft')",values)),
requireAuthorityCurrent:(key,values)=>stage(assertion(db,key,"authority-current","EXISTS(SELECT 1 FROM legal_authorities WHERE id=? AND version=?)",values)),
guardChanges:(id,version,key,edit)=>guards(db,a,id,version,key,edit).map(stage),companyLock:()=>stage(lock(db,a.companyId)),personGuard:(key,id,item,permission)=>stage(personGuard(db,a,key,id,item,permission)),
event:(id,action,note,version,identifier,now)=>stage(db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(identifier,id,action,a.userId,note,version,now)),
replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_REPOSITORY_MISMATCH");return statement;})),
}});}
export async function governanceView(db:SqlDatabase,a:WorkContext,id?:string){if(await legalPackageEnforcementMode()==="disabled")return application(db,a).governanceView(id);if(id)await requireLegalItemPurchasedRead(db,a,await access(db,a,id),["legal.access","legal.documents"]);const purchases=await legalPurchaseControls(db,a),data=await application(db,a).governanceView(id);return {...data,...(purchases?.["legal.matters"]?.history===false?{forms:[]}:{}),purchases};}
export async function governanceCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 const input=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>,action=String(input.action),form=action.startsWith('form.');
 if(!form&&await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).governanceCommand(raw,key));
 return withLegalPurchasedCommand(db,a,{key,id:form?undefined:typeof input.id==='string'?input.id:undefined,operation:action,purpose:['form.create','intake.apply','authority.create'].includes(action)?'new-work':'settle-existing',permission:form?'legal.manage':action==='authority.approve'?'legal.approve':'legal.edit',additional:form||action==='intake.apply'?['legal.matters']:['legal.access','legal.documents'],ownerSource:form?{table:'legal_intake_forms',id:action==='form.create'?key:String(input.formId)}:undefined},tx=>application(tx,a).governanceCommand(raw,key));
}
