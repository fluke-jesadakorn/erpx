import {legalPackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {withLegalPurchasedCommand,requireLegalItemPurchasedRead,inLegacyLegalOperationsUnit} from '@erpx/server-legal/adapters/purchases';
import {samePerson as samePrincipal} from '@erpx/feature-identity_access/lib/iam';
import "server-only";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {access,guards,eligible,personGuard,lock} from "@erpx/server-legal/adapters/legal/access";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import {featureSpecs,featureKinds,type FeatureRow} from "@erpx/feature-legal/lib/legal/features";
import {createLegalFeaturesApplication} from "@erpx/server-legal/application/features";
import { legalFeatureStatements } from '@erpx/server-collaboration/adapters/native-legal-features';
import {legalMembershipSql} from '../legal-members-read-sources';
import {legalDocumentsItemPairSql} from '../legal-read-tail-r9-sources';
import {legalByIdAndItemSql} from '../legal-read-tail-r27-sources';
import {legalPointByIdAndKeySql} from '../legal-read-tail-r34-sources';
import {legalFeatureSearchSql} from '../legal-read-tail-r42-sources';
import {legalCounselConflictClearedSql,legalCounselApprovedFeesSql} from '../legal-read-tail-r56-sources';
function application(db:SqlDatabase,a:WorkContext){const nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const nativeFeatures=legalFeatureStatements(db,a.companyId,a.userId);const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
return createLegalFeaturesApplication({actor:a,access:(id,p)=>access(db,a,id,p),eligible:(id,item,p)=>eligible(db,a,id,item,p),samePerson:id=>samePerson(a,id),require:permission=>requirePermission(a,permission),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
featureCount:(facts:{id:string;search:string;search2:string;capability:(typeof featureKinds)[number];})=>db.prepare(legalFeatureSearchSql("count(*) n",featureSpecs[facts.capability].table)).bind(facts.id,facts.search,facts.search2).first("n"),
features:(facts:{id:string;search:string;search2:string;fact3:number;capability:(typeof featureKinds)[number];})=>db.prepare(legalFeatureSearchSql("*",featureSpecs[facts.capability].table)+" ORDER BY updated_at DESC,id LIMIT 50 OFFSET ?").bind(facts.id,facts.search,facts.search2,facts.fact3).all<FeatureRow>(),
feature:(facts:{featureId:string;itemId:string;capability:(typeof featureKinds)[number];})=>db.prepare(legalByIdAndItemSql(featureSpecs[facts.capability].table)).bind(facts.featureId,facts.itemId).first<FeatureRow>(),
ownerMembership:(facts:{itemId:string;ownerId:string;})=>db.prepare(legalMembershipSql).bind(facts.itemId,facts.ownerId).first(),
attachedDocument:(facts:{documentId:string;itemId:string;})=>db.prepare(legalDocumentsItemPairSql).bind(facts.documentId,facts.itemId).first(),
async createFeature(facts){return stage(await nativeFeatures.createFeature({...facts,table:featureSpecs[facts.capability].table}));},
conflictCleared:(facts:{itemId:string;})=>db.prepare(legalCounselConflictClearedSql()).bind(facts.itemId).first(),
approvedFees:(facts:{itemId:string;id:string;})=>db.prepare(legalCounselApprovedFeesSql()).bind(facts.itemId,facts.id).first<{ amount: number }>(),
async updateFeature(facts){return stage(await nativeFeatures.updateFeature({...facts,table:featureSpecs[facts.capability].table}));},
async recordFeatureHistory(facts){return stage(await nativeFeatures.recordFeatureHistory(facts));},
async advanceRecord(facts){return stage(await nativeFeatures.advanceRecord(facts));},
guardChanges:(id,version,key,edit)=>guards(db,a,id,version,key,edit).map(stage),companyLock:()=>stage(lock(db,a.companyId)),personGuard:(key,id,item,permission)=>stage(personGuard(db,a,key,id,item,permission)),
async event(id,action,note,version,identifier,now){return stage(await nativeFeatures.event({id,action,note,version,identifier,now}));},
replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_REPOSITORY_MISMATCH");return statement;})),
}});}
export async function featureDetail(db:SqlDatabase,a:WorkContext,id:string,query:Record<string,unknown>={}){if(await legalPackageEnforcementMode()!=="disabled")await requireLegalItemPurchasedRead(db,a,await access(db,a,id),["legal.obligations"]);return application(db,a).featureDetail(id,query);}
export async function featureCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 if(await legalPackageEnforcementMode()==="disabled")return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).featureCommand(raw,key));
 const input=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>,spec=featureSpecs[input.category as keyof typeof featureSpecs],id=typeof input.id==='string'?input.id:undefined;
 const row=spec&&typeof input.featureId==='string'?await db.prepare(legalPointByIdAndKeySql("owner_id",spec.table,"item_id")).bind(input.featureId,id??null).first<{owner_id:string}>():null,ownerResponse=row&&samePrincipal(a,row.owner_id)&&['intake','policy_ack'].includes(String(input.category))&&input.action!=='feature.create';
 return withLegalPurchasedCommand(db,a,{key,id,operation:String(input.action),purpose:input.action==='feature.transition'?'settle-existing':'new-work',permission:ownerResponse?'legal.read':'legal.edit',additionalPermissions:(spec?.independentStates as readonly string[]|undefined)?.includes(String(input.state))?['legal.approve']:[],additional:['legal.obligations']},tx=>application(tx,a).featureCommand(raw,key));
}
