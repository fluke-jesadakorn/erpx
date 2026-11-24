import {legalPackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {withLegalPurchasedCommand,requireLegalItemPurchasedRead,inLegacyLegalOperationsUnit} from '@erpx/server-legal/adapters/purchases';
import { finalRootOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/final-root-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {legalEvidenceSamePerson} from "@erpx/server-identity_access/public/legal-evidence-identity";
import {ApiError} from "@erpx/server-runtime/public/http";
import {access,eligible,guards,personGuard} from "@erpx/server-legal/adapters/legal/access";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {legalLaterApprovalGuardSql} from "../legal-read-tail-r15-sources";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import type {ExecutedCopy} from "@erpx/server-legal/domain/model";
import {createExecutedCopyApplication,type ExecutedCopySource} from "@erpx/server-legal/application/executed-copies";
import { legalExecutedCopies } from '@erpx/server-collaboration/adapters/native-legal-executed-copies';
import {legalMembershipSql,legalMembershipExistsSql} from '../legal-members-read-sources';
function application(db:SqlDatabase,a:WorkContext){const nativeExec=legalExecutedCopies(db,a.companyId,a.userId);return createExecutedCopyApplication({access:(id,permission)=>access(db,a,id,permission),eligible:item=>eligible(db,a,a.userId,item,"legal.approve"),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
list:async id=>(await db.prepare("SELECT e.*,d.name document_name,d.version document_version FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id WHERE e.item_id=? ORDER BY e.imported_at DESC,e.id LIMIT 100").bind(id).all<ExecutedCopy&{document_name:string;document_version:number}>()).results,
reviews:async id=>(await db.prepare("SELECT * FROM legal_executed_reviews WHERE executed_copy_id=? ORDER BY created_at,id").bind(id).all()).results,
member:async id=>Boolean(await db.prepare(legalMembershipSql).bind(id,a.userId).first()),
replay:(key,id,input)=>replayCommand(db,a,key,"legal.executed.review",id,input),
row:(id,itemId)=>db.prepare("SELECT e.*,d.created_by document_author,p.created_by approved_author FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id WHERE e.id=? AND e.item_id=?").bind(id,itemId).first<ExecutedCopySource>(),
samePerson:author=>legalEvidenceSamePerson(db,a.companyId,a.userId,author),
approval:id=>db.prepare("SELECT cycle FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1").bind(id).first<{cycle:number}>(),
commit:async(x,item,e,authors,state,now,event,key)=>{
  return executeCommand(db,a,key,"legal.executed.review",x,item.id,item.version+1,{id:item.id,version:item.version+1,state,executedCopyId:e.id,executedCopyState:x.decision==="confirm"?"confirmed":"changes_required"},[
    ...guards(db,a,item.id,item.version,key),
    personGuard(db,a,key,a.userId,item,"legal.approve"),
    assertion(db,key,"legal-executed-reviewer",legalMembershipExistsSql,[item.id,a.userId]),
    ...authors.map((author,i)=>assertion(db,key,`legal-executed-independent-${i}`,"NOT iam_same_person(?,?,?)",[a.userId,author,a.companyId])),
    assertion(db,key,"legal-executed-current",`EXISTS(SELECT 1 FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id JOIN (${filesProjectionSources.attachments}) f ON f.id=d.attachment_source_id WHERE e.id=? AND e.version=? AND e.state!='confirmed' AND d.checksum=e.executed_hash AND p.checksum=e.approved_hash AND p.locked=true AND f.source_state='available') AND NOT EXISTS(SELECT 1 FROM legal_disposals WHERE document_id IN (?,?)) AND ${legalLaterApprovalGuardSql}`,[e.id,e.version,e.document_id,e.approved_document_id,item.id,e.approved_cycle]),
    await nativeExec.review({ key, executedCopyId: e.id, itemVersion: item.version, decision: x.decision, checks: JSON.stringify(x.checks), reason: x.reason, now }),
    await nativeExec.copyState({ state: x.decision==="confirm"?"confirmed":"changes_required", reviewedBy: x.decision==="confirm"?a.userId:null, reviewedAt: x.decision==="confirm"?now:null, id: e.id }),
    await nativeExec.advanceContract({ state, now, itemId: item.id }),
    await nativeExec.event({ id: event.id, action: `executed.${x.decision}`, note: `${e.id} · ${x.reason}`, version: item.version+1, identifier: item.id, now: event.now }),
  ]);
}
}});}
export async function executedCopies(db:SqlDatabase,a:WorkContext,id:string){if(await legalPackageEnforcementMode()!=="disabled")await requireLegalItemPurchasedRead(db,a,await access(db,a,id),["legal.documents"]);return application(db,a).executedCopies(id);}
export async function reviewExecutedCopy(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 if(await legalPackageEnforcementMode()==='disabled')return inLegacyLegalOperationsUnit(db,a.companyId,tx=>application(tx,a).reviewExecutedCopy(raw,key));
 const id=raw&&typeof raw==="object"&&"id" in raw&&typeof raw.id==="string"?raw.id:undefined;return withLegalPurchasedCommand(db,a,{key,id,operation:"legal.executed.review",purpose:"settle-existing",permission:"legal.approve",additional:["legal.documents"]},tx=>application(tx,a).reviewExecutedCopy(raw,key));
}
