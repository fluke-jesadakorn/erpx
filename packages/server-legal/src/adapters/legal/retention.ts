import {recordLegalRetentionExecution} from "@erpx/server-legal/adapters/retention-execution";
import {legalPackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {withLegalPurchasedCommand,requireLegalItemPurchasedRead,legalPurchaseControls,inLegacyLegalOperationsUnit} from '@erpx/server-legal/adapters/purchases';
import "server-only";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {ApiError} from "@erpx/server-runtime/public/http";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {access,guards,lock} from "@erpx/server-legal/adapters/legal/access";
import {legalDestructionManifest} from "@erpx/server-legal/adapters/legal/destruction-sources";
import {createLegalRetentionApplication} from "@erpx/server-legal/application/retention";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import { legalRetention } from '@erpx/server-collaboration/adapters/native-legal-retention';
import {nativeLegalAuxiliaryRead} from '../native-legal-auxiliary';
import {legalHoldGuardPrefix,legalHoldGuardSuffix} from '../legal-hold-guard-sources';
import {legalRetentionPolicySelectSql} from '../legal-read-tail-r18-sources';
export {drainLegalDestruction} from "@erpx/server-legal/adapters/legal/destruction-worker";
/** Exact projection of the native `retention` auxiliary read; the transport accepts
 * every authoritative field and no SQL text remains in TypeScript. */
type LegalRetentionRead={policies:Record<string,unknown>[];assignment:(Record<string,unknown>&{id:string})|null;requests:(Record<string,unknown>&{id:string})[];};
function application(db:SqlDatabase,a:WorkContext,view?:LegalRetentionRead){const nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const nativeRet=legalRetention(db,a.companyId,a.userId);const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};return createLegalRetentionApplication({actor:a,access:(id,p)=>access(db,a,id,p),require:p=>requirePermission(a,p),samePerson:id=>samePerson(a,id),manifest:(id,company)=>legalDestructionManifest(db,id,company),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
policies:async()=>({results:view?.policies??[]}),
assignment:async()=>view?.assignment??null,
requests:async()=>({results:view?.requests??[]}),
async createPolicy(facts){return stage(await nativeRet.createPolicy(facts));},
policy:(facts:{policyId:string | undefined;companyId:string;})=>db.prepare(legalRetentionPolicySelectSql).bind(facts.policyId,facts.companyId).first<{
          id: string;
          created_by: string;
          state: string;
          version: number;
        }>(),
async decidePolicy(facts){return stage(await nativeRet.decidePolicy(facts));},
approvedPolicy:(facts:{policyId:string | undefined;companyId:string;})=>db.prepare(`${legalRetentionPolicySelectSql} AND state='approved'`).bind(facts.policyId,facts.companyId).first<{ id: string; days: number; version: number }>(),
async assignRetention(facts){return stage(await nativeRet.assignRetention(facts));},
eligibleAssignment:(facts:{id:string;now:number;})=>db.prepare("SELECT r.* FROM legal_retention_assignments r JOIN legal_retention_policies p ON p.id=r.policy_id AND p.version=r.policy_version AND p.state='approved' WHERE r.item_id=? AND r.eligible_at<=?").bind(facts.id,facts.now).first<{ policy_id: string; policy_version: number }>(),
async requestDestruction(facts){return stage(await nativeRet.requestDestruction(facts));},
destruction:(facts:{requestId:string | undefined;id:string;})=>db.prepare("SELECT * FROM legal_destructions WHERE id=? AND item_id=?").bind(facts.requestId,facts.id).first<{
        id: string;
        state: string;
        requested_by: string;
        approved_by: string | null;
      }>(),
async changeDestruction(facts){return stage(await nativeRet.changeDestruction(facts));},
async advanceRecord(facts){return stage(await nativeRet.advanceRecord({now:facts.now,itemId:facts.id}));},
requireRetentionPolicyVersion:(key,values)=>stage(assertion(db,key,"retention-policy-version","EXISTS(SELECT 1 FROM legal_retention_policies WHERE id=? AND version=?)",values)),
requireRetentionNoHold:(key,values)=>stage(assertion(db,key,"retention-no-hold",legalHoldGuardPrefix+"?"+legalHoldGuardSuffix,values)),
guardChanges:(id,version,key,edit,allowFrozen)=>guards(db,a,id,version,key,edit,allowFrozen).map(stage),companyLock:()=>stage(lock(db,a.companyId)),event:async(id,action,note,version,identifier,now)=>stage(await nativeRet.event({id,action,note,version,identifier,now})),replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),commit:async(key,operation,input,id,version,result,writes)=>{const receipt=await executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_RETENTION_REPOSITORY_MISMATCH");return statement;}));await recordLegalRetentionExecution(db,a,key,operation,input);return receipt;},
}});}
export async function retentionView(db:SqlDatabase,a:WorkContext,id?:string){requirePermission(a,"legal.read");if(await legalPackageEnforcementMode()!=="disabled"){if(id)await requireLegalItemPurchasedRead(db,a,await access(db,a,id),["legal.obligations"]);else if((await legalPurchaseControls(db,a))?.["legal.obligations"]?.history===false)throw new ApiError(403,"PACKAGE_REQUIRED","Legal obligations history is required.");}const view=await nativeLegalAuxiliaryRead<LegalRetentionRead>(a,'retention',id);return application(db,a,view).retentionView(id);}
export async function retentionCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).retentionCommand(raw,key));
 const input=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>,policy=['retention.policy','retention.approve-policy','retention.retire-policy'].includes(String(input.action));return withLegalPurchasedCommand(db,a,{key,id:policy?undefined:typeof input.id==='string'?input.id:undefined,operation:String(input.action),purpose:input.action==='retention.policy'?'new-work':'history',permission:policy?'legal.manage':'legal.destroy',additional:['legal.obligations'],ownerSource:policy?{table:'legal_retention_policies',id:input.action==='retention.policy'?key:String(input.policyId)}:undefined},tx=>application(tx,a).retentionCommand(raw,key));
}
