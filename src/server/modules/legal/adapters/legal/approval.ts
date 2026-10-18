import {withLegalPurchasedCommand,inLegacyLegalOperationsUnit} from '../purchases';
import {legalPackageEnforcementMode} from '../../../commercial/public/purchase-command-boundary';
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../../identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as workflowProjectionSources } from "../../../workflow/public/final-root-owner-projection-sources";
import "server-only";
import {createHash} from "node:crypto";
import {samePerson} from "@/lib/identity_access/iam";
import type {SqlDatabase,SqlStatement} from "../../../runtime/public/runtime/types";
import {ApiError} from "../../../runtime/public/http";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {legalEvidenceSamePerson} from "../../../identity_access/public/legal-evidence-identity";
import {legalApprovalFacts} from "../../../workflow/public/legal-approval-facts";
import {legalApprovalNotifications} from "../../../collaboration/public/legal-approval-notifications";
import {openApproval,currentApproval} from "../../../workflow/public/approvals";
import {executeCommand,replayCommand,assertion} from "../../../workflow/public/commands";
import {access,guards,eligible,personGuard} from "./access";
import type {DocumentVersion,LegalItem} from "../../domain/model";
import {createLegalApprovalApplication} from "../../application/approval";
import type {LegalWrite} from "../../application/service";
function application(db:SqlDatabase,a:WorkContext){const workflow=legalApprovalFacts(db),collaboration=legalApprovalNotifications(db),nativeWrites=new WeakMap<LegalWrite,SqlStatement>();const stage=(statement:SqlStatement):LegalWrite=>{const write:LegalWrite={owner:"legal"};nativeWrites.set(write,statement);return write;};
return createLegalApprovalApplication({actor:a,access:(id,p)=>access(db,a,id,p),eligible:(id,item,p)=>eligible(db,a,id,item,p),samePerson:id=>samePerson(a,id),hash:value=>createHash("sha256").update(value).digest("hex"),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
routeSteps:facts=>workflow.routeSteps(facts),
decisionHistory:facts=>workflow.decisionHistory(facts),
contractDocument:(facts:{documentId:string;itemId:string;})=>db.prepare("SELECT * FROM legal_documents WHERE id=? AND item_id=?").bind(facts.documentId,facts.itemId).first<DocumentVersion>(),
executedDocument:(facts:{id:string;})=>db.prepare("SELECT 1 FROM legal_executed_copies WHERE document_id=?").bind(facts.id).first(),
latestAuthor:(facts:{itemId:string;})=>db.prepare("SELECT actor_id FROM legal_events WHERE item_id=? AND action IN ('created','update','document','document.upload','ai.accepted') ORDER BY version DESC LIMIT 1").bind(facts.itemId).first<{ actor_id: string }>(),
signerAccess:(facts:{itemId:string;u:string;itemId2:string;id:string;u2:string;now:number;})=>db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=? UNION ALL SELECT 1 FROM legal_shares WHERE item_id=? AND document_id=? AND user_id=? AND revoked_at IS NULL AND expires_at>?").bind(facts.itemId,facts.u,facts.itemId2,facts.id,facts.u2,facts.now).first(),
lockDocument:(facts:{id:string;})=>stage(db.prepare("UPDATE legal_documents SET locked=true WHERE id=?").bind(facts.id)),
retainRoute:(facts:{itemId:string;instanceId:string;id:string;checksum:string;signerIdsJson:string;nextVersion:number;actorId:string;sourceHash:string;})=>stage(db.prepare("INSERT INTO legal_approval_routes(item_id,instance_id,document_id,document_hash,signers,source_version,created_by,source_hash) VALUES(?,?,?,?,?::jsonb,?,?,?) ON CONFLICT(item_id) DO UPDATE SET instance_id=excluded.instance_id,document_id=excluded.document_id,document_hash=excluded.document_hash,signers=excluded.signers,source_version=excluded.source_version,created_by=excluded.created_by,source_hash=excluded.source_hash").bind(facts.itemId,facts.instanceId,facts.id,facts.checksum,facts.signerIdsJson,facts.nextVersion,facts.actorId,facts.sourceHash)),
currentStep:facts=>workflow.currentStep(facts),
routeSnapshot:(facts:{itemId:string;instanceId:string;})=>db.prepare("SELECT * FROM legal_approval_routes WHERE item_id=? AND instance_id=?").bind(facts.itemId,facts.instanceId).first<{
        document_id: string;
        document_hash: string;
        signers: string;
        source_hash: string;
      }>(),
documentAuthor:(facts:{document_id:string;})=>db.prepare("SELECT created_by FROM legal_documents WHERE id=?").bind(facts.document_id).first<{ created_by: string }>(),
advanceRoute:facts=>stage(workflow.advanceRoute(facts)),
replaceReviewer:facts=>stage(workflow.replaceReviewer(facts)),
recordReroute:facts=>stage(workflow.recordReroute(facts)),
decideStep:facts=>stage(workflow.decideStep(facts)),
recordDecision:facts=>stage(workflow.recordDecision(facts)),
nextReviewer:facts=>workflow.nextReviewer(facts),
cancelRoute:facts=>stage(workflow.cancelRoute(facts)),
advanceStep:facts=>stage(workflow.advanceStep(facts)),
notifyReviewer:facts=>stage(collaboration.notifyReviewer(facts)),
approveRoute:facts=>stage(workflow.approveRoute(facts)),
recordApproval:(facts:{itemId:string;nextVersion:number;document_id:string;document_hash:string;signerIdsJson:string;actorId:string;now:number;})=>stage(db.prepare("INSERT INTO legal_approvals VALUES(?,?,?,?,?::jsonb,?,'approved',?)").bind(facts.itemId,facts.nextVersion,facts.document_id,facts.document_hash,facts.signerIdsJson,facts.actorId,facts.now)),
advanceContract:(facts:{state:string;now:number;itemId:string;})=>stage(db.prepare("UPDATE legal_items SET state=?,version=version+1,updated_at=? WHERE id=?").bind(facts.state,facts.now,facts.itemId)),
reviewerMembership:(facts:{company_id:string;itemId:string;id:string;now:number;now2:number;})=>db.prepare(`SELECT 1 FROM legal_members m JOIN (${identity_accessProjectionSources.assignments}) a ON a.user_id=m.user_id AND a.company_id=? WHERE m.item_id=? AND m.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_role_allows(a.role,a.company_id,'legal.approve')`).bind(facts.company_id,facts.itemId,facts.id,facts.now,facts.now2).first(),
sameAuthor:facts=>legalEvidenceSamePerson(db,facts.company,facts.user,facts.author).then(value=>value?{}:null),
requireReviewerMembership:(key:string,reviewerId:string,itemId:string)=>stage(assertion(db,key,`legal-reviewer-membership-${reviewerId}`,"EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[itemId,reviewerId])),
requireCurrentRoute:(key:string,instanceId:string,currentStep:number,version:number)=>stage(assertion(db,key,"legal-approval-current",`EXISTS(SELECT 1 FROM (${workflowProjectionSources.approval_instances}) approval_instances WHERE id=? AND state='pending' AND current_step=? AND version=?)`,[instanceId,currentStep,version])),
requireReplacementMembership:(key:string,itemId:string,reviewerId:string)=>stage(assertion(db,key,"legal-reroute-membership","EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[itemId,reviewerId])),
requireDecisionMembership:(key:string,itemId:string)=>stage(assertion(db,key,"legal-decision-membership","EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[itemId,a.userId])),
currentApproval:(id,companyId)=>currentApproval(db,"legal",id,companyId),
openApproval:async(key,recordType,id,policyKey,requesterId,subjectId,route,version)=>{const opened=await openApproval(db,key,recordType,id,policyKey,requesterId,subjectId,route,version);return {...opened,statements:opened.statements.map(stage)};},
guardChanges:(id,version,key,edit)=>guards(db,a,id,version,key,edit).map(stage),personGuard:(key,id,item,permission)=>stage(personGuard(db,a,key,id,item,permission)),
event:(id,action,note,version,identifier,now)=>stage(db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(identifier,id,action,a.userId,note,version,now)),
replay:(key,operation,id,input)=>replayCommand(db,a,key,operation,id,input),
commit:(key,operation,input,id,version,result,writes)=>executeCommand(db,a,key,operation,input,id,version,result,writes.map(write=>{const statement=nativeWrites.get(write);if(!statement)throw new Error("LEGAL_APPROVAL_REPOSITORY_MISMATCH");return statement;})),
}});}
export function legalApprovalView(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).legalApprovalView(id);}
export async function legalApprovalCommand(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).legalApprovalCommand(raw,key));
 const input=(raw&&typeof raw==='object'?raw:{}) as Record<string,unknown>;return withLegalPurchasedCommand(db,a,{key,operation:String(input.action),id:typeof input.id==='string'?input.id:undefined,purpose:'settle-existing',permission:input.action==='approval.open'?'legal.review':'legal.approve',additional:['legal.documents']},tx=>application(tx,a).legalApprovalCommand(raw,key));
}
export function approvalDigest(item:LegalItem,documentId:string,checksum:string,signers:string[]){const {version:_version,updated_at:_updated,...source}=item;void _version;void _updated;return createHash("sha256").update(JSON.stringify({source,documentId,checksum,signers})).digest("hex");}
