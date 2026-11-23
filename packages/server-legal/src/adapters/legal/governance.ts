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
import { legalGovernance } from '@erpx/server-collaboration/adapters/native-legal-governance';
import {nativeLegalAuxiliaryRead} from '../native-legal-auxiliary';
/** Exact projection of the native `governance` auxiliary read; the transport accepts
 * every authoritative field and no SQL text remains in TypeScript. */
type LegalGovernanceRead={forms:Record<string,unknown>[];authorities:Record<string,unknown>[];settings:(Record<string,unknown>&{enabled:boolean;ai_enabled:boolean;reminder_days:number})|null;};
function application(db:SqlDatabase,a:WorkContext,view?:LegalGovernanceRead){const nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const nativeGov=legalGovernance(db,a.companyId,a.userId);const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
return createLegalGovernanceApplication({actor:a,access:(id,p)=>access(db,a,id,p),eligible:(id,item,p)=>eligible(db,a,id,item,p),samePerson:id=>samePerson(a,id),require:permission=>requirePermission(a,permission),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
forms:async()=>({results:view?.forms??[]}),
authorities:async()=>({results:view?.authorities??[]}),
settings:async()=>view?.settings??null,
async createForm(facts){return stage(await nativeGov.createForm(facts));},
form:(facts:{formId:string;companyId:string;})=>db.prepare("SELECT * FROM legal_intake_forms WHERE id=? AND company_id=?").bind(facts.formId,facts.companyId).first<{ created_by: string; state: string; version: number }>(),
async approveForm(facts){return stage(await nativeGov.approveForm(facts));},
approvedForm:(facts:{formId:string;companyId:string;})=>db.prepare("SELECT fields,sla_days FROM legal_intake_forms WHERE id=? AND company_id=? AND state='approved'").bind(facts.formId,facts.companyId).first<{ fields: string; sla_days: number }>(),
escalationMember:(facts:{itemId:string;escalationOwner:string;})=>db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?").bind(facts.itemId,facts.escalationOwner).first(),
async applyIntake(facts){return stage(await nativeGov.applyIntake(facts));},
authorityDocument:(facts:{documentId:string;itemId:string;})=>db.prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=?").bind(facts.documentId,facts.itemId).first(),
signerAccess:(facts:{itemId:string;userId:string;itemId2:string;userId2:string;now:number;})=>db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? UNION ALL SELECT 1 FROM legal_shares WHERE item_id=? AND user_id=? AND revoked_at IS NULL AND expires_at>?").bind(facts.itemId,facts.userId,facts.itemId2,facts.userId2,facts.now).first(),
async createAuthority(facts){return stage(await nativeGov.createAuthority(facts));},
authority:(facts:{authorityId:string;itemId:string;})=>db.prepare("SELECT * FROM legal_authorities WHERE id=? AND item_id=?").bind(facts.authorityId,facts.itemId).first<{ created_by: string; state: string; version: number }>(),
async decideAuthority(facts){return stage(await nativeGov.decideAuthority(facts));},
async advanceRecord(facts){return stage(await nativeGov.advanceRecord(facts));},
requireFormCurrent:(key,values)=>stage(assertion(db,key,"form-current","EXISTS(SELECT 1 FROM legal_intake_forms WHERE id=? AND version=? AND state='draft')",values)),
requireAuthorityCurrent:(key,values)=>stage(assertion(db,key,"authority-current","EXISTS(SELECT 1 FROM legal_authorities WHERE id=? AND version=?)",values)),
guardChanges:(id,version,key,edit)=>guards(db,a,id,version,key,edit).map(stage),companyLock:()=>stage(lock(db,a.companyId)),personGuard:(key,id,item,permission)=>stage(personGuard(db,a,key,id,item,permission)),
async event(id,action,note,version,identifier,now){return stage(await nativeGov.event({id,action,note,version,identifier,now}));},
replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_REPOSITORY_MISMATCH");return statement;})),
}});}
export async function governanceView(db:SqlDatabase,a:WorkContext,id?:string){if(await legalPackageEnforcementMode()==="disabled"){const view=await nativeLegalAuxiliaryRead<LegalGovernanceRead>(a,'governance',id);return application(db,a,view).governanceView(id);}if(id)await requireLegalItemPurchasedRead(db,a,await access(db,a,id),["legal.access","legal.documents"]);const purchases=await legalPurchaseControls(db,a),view=await nativeLegalAuxiliaryRead<LegalGovernanceRead>(a,'governance',id),data=await application(db,a,view).governanceView(id);return {...data,...(purchases?.["legal.matters"]?.history===false?{forms:[]}:{}),purchases};}
export async function governanceCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 const input=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>,action=String(input.action),form=action.startsWith('form.');
 if(!form&&await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).governanceCommand(raw,key));
 return withLegalPurchasedCommand(db,a,{key,id:form?undefined:typeof input.id==='string'?input.id:undefined,operation:action,purpose:['form.create','intake.apply','authority.create'].includes(action)?'new-work':'settle-existing',permission:form?'legal.manage':action==='authority.approve'?'legal.approve':'legal.edit',additional:form||action==='intake.apply'?['legal.matters']:['legal.access','legal.documents'],ownerSource:form?{table:'legal_intake_forms',id:action==='form.create'?key:String(input.formId)}:undefined},tx=>application(tx,a).governanceCommand(raw,key));
}
