import {withLegalPurchasedCommand,inLegacyLegalOperationsUnit} from '@erpx/server-legal/adapters/purchases';
import {legalPackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/final-root-owner-projection-sources";
import "server-only";
import {createHash} from "node:crypto";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {legalEvidenceSamePerson} from "@erpx/server-identity_access/public/legal-evidence-identity";
import {legalApprovalFacts} from "@erpx/server-workflow/public/legal-approval-facts";
import {nativeLegalApprovalNotification} from "@erpx/server-collaboration/adapters/native-legal-approval-notification";
import {openApproval,currentApproval} from '@erpx/server-workflow/public/approval-planning';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {access,guards,eligible,personGuard} from "@erpx/server-legal/adapters/legal/access";
import type {DocumentVersion,LegalItem} from "@erpx/server-legal/domain/model";
import {createLegalApprovalApplication} from "@erpx/server-legal/application/approval";
import type {LegalWrite} from "@erpx/server-legal/application/service";
import { legalApprovalStatements } from '@erpx/server-collaboration/adapters/native-legal-approval';
import {legalMembershipExistsSql} from '../legal-members-read-sources';
import {legalSignerAccessPrefix,legalSignerAccessSuffix} from '../legal-signer-access-sources';
import {legalByIdAndItemSql} from '../legal-read-tail-r27-sources';
import {legalPointByIdSql} from '../legal-read-tail-r32-sources';
import {legalPointByKeySql} from '../legal-read-tail-r36-sources';
function application(db:SqlDatabase,a:WorkContext){const workflow=legalApprovalFacts(db),nativeWrites=new WeakMap<LegalWrite,SqlStatement>(),nativeApproval=legalApprovalStatements(db,a.companyId,a.userId);const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
return createLegalApprovalApplication({actor:a,access:(id,p)=>access(db,a,id,p),eligible:(id,item,p)=>eligible(db,a,id,item,p),samePerson:id=>samePerson(a,id),hash:value=>createHash("sha256").update(value).digest("hex"),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
routeSteps:facts=>workflow.routeSteps(facts),
decisionHistory:facts=>workflow.decisionHistory(facts),
contractDocument:(facts:{documentId:string;itemId:string;})=>db.prepare(legalByIdAndItemSql("legal_documents")).bind(facts.documentId,facts.itemId).first<DocumentVersion>(),
executedDocument:(facts:{id:string;})=>db.prepare(legalPointByKeySql("1","legal_executed_copies","document_id")).bind(facts.id).first(),
latestAuthor:(facts:{itemId:string;})=>db.prepare("SELECT actor_id FROM legal_events WHERE item_id=? AND action IN ('created','update','document','document.upload','ai.accepted') ORDER BY version DESC LIMIT 1").bind(facts.itemId).first<{ actor_id: string }>(),
signerAccess:(facts:{itemId:string;u:string;itemId2:string;id:string;u2:string;now:number;})=>db.prepare(legalSignerAccessPrefix+"document_id=? AND user_id=?"+legalSignerAccessSuffix).bind(facts.itemId,facts.u,facts.itemId2,facts.id,facts.u2,facts.now).first(),
async lockDocument(facts){return stage(await nativeApproval.lockDocument(facts));},
async retainRoute(facts){return stage(await nativeApproval.retainRoute(facts));},
currentStep:facts=>workflow.currentStep(facts),
routeSnapshot:(facts:{itemId:string;instanceId:string;})=>db.prepare("SELECT * FROM legal_approval_routes WHERE item_id=? AND instance_id=?").bind(facts.itemId,facts.instanceId).first<{
        document_id: string;
        document_hash: string;
        signers: string;
        source_hash: string;
      }>(),
documentAuthor:(facts:{document_id:string;})=>db.prepare(legalPointByIdSql("created_by","legal_documents")).bind(facts.document_id).first<{ created_by: string }>(),
advanceRoute:facts=>stage(workflow.advanceRoute(facts)),
replaceReviewer:facts=>stage(workflow.replaceReviewer(facts)),
recordReroute:facts=>stage(workflow.recordReroute(facts)),
decideStep:facts=>stage(workflow.decideStep(facts)),
recordDecision:facts=>stage(workflow.recordDecision(facts)),
nextReviewer:facts=>workflow.nextReviewer(facts),
cancelRoute:facts=>stage(workflow.cancelRoute(facts)),
advanceStep:facts=>stage(workflow.advanceStep(facts)),
notifyReviewer:async facts=>stage(await nativeLegalApprovalNotification(db,a,facts)),
approveRoute:facts=>stage(workflow.approveRoute(facts)),
async recordApproval(facts){return stage(await nativeApproval.recordApproval(facts));},
async advanceContract(facts){return stage(await nativeApproval.advanceContract(facts));},
reviewerMembership:(facts:{company_id:string;itemId:string;id:string;now:number;now2:number;})=>db.prepare(`SELECT 1 FROM legal_members m JOIN (${identity_accessProjectionSources.assignments}) a ON a.user_id=m.user_id AND a.company_id=? WHERE m.item_id=? AND m.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_role_allows(a.role,a.company_id,'legal.approve')`).bind(facts.company_id,facts.itemId,facts.id,facts.now,facts.now2).first(),
sameAuthor:facts=>legalEvidenceSamePerson(db,facts.company,facts.user,facts.author).then(value=>value?{}:null),
requireReviewerMembership:(key:string,reviewerId:string,itemId:string)=>stage(assertion(db,key,`legal-reviewer-membership-${reviewerId}`,legalMembershipExistsSql,[itemId,reviewerId])),
requireCurrentRoute:(key:string,instanceId:string,currentStep:number,version:number)=>stage(assertion(db,key,"legal-approval-current",`EXISTS(SELECT 1 FROM (${workflowProjectionSources.approval_instances}) approval_instances WHERE id=? AND state='pending' AND current_step=? AND version=?)`,[instanceId,currentStep,version])),
requireReplacementMembership:(key:string,itemId:string,reviewerId:string)=>stage(assertion(db,key,"legal-reroute-membership",legalMembershipExistsSql,[itemId,reviewerId])),
requireDecisionMembership:(key:string,itemId:string)=>stage(assertion(db,key,"legal-decision-membership",legalMembershipExistsSql,[itemId,a.userId])),
currentApproval:(id,companyId)=>currentApproval(db,"legal",id,companyId),
openApproval:async(key,recordType,id,policyKey,requesterId,subjectId,route,version)=>{const opened=await openApproval(db,key,recordType,id,policyKey,requesterId,subjectId,route,version);return {...opened,statements:opened.statements.map(stage)};},
guardChanges:(id,version,key,edit)=>guards(db,a,id,version,key,edit).map(stage),personGuard:(key,id,item,permission)=>stage(personGuard(db,a,key,id,item,permission)),
async event(id,action,note,version,identifier,now){return stage(await nativeApproval.event({id,action,note,version,identifier,now}));},
replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_APPROVAL_REPOSITORY_MISMATCH");return statement;})),
}});}
export function legalApprovalView(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).legalApprovalView(id);}
export async function legalApprovalCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).legalApprovalCommand(raw,key));
 const input=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>;return withLegalPurchasedCommand(db,a,{key,operation:String(input.action),id:typeof input.id==='string'?input.id:undefined,purpose:'settle-existing',permission:input.action==='approval.open'?'legal.review':'legal.approve',additional:['legal.documents']},tx=>application(tx,a).legalApprovalCommand(raw,key));
}
export function approvalDigest(item:LegalItem,documentId:string,checksum:string,signers:string[]){const {version:_version,updated_at:_updated,...source}=item;void _version;void _updated;return createHash("sha256").update(JSON.stringify({source,documentId,checksum,signers})).digest("hex");}
