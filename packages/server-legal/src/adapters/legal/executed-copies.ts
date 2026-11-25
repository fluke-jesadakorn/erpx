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
import {legalDisposedDocumentPrefix} from "../legal-read-tail-r20-sources";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {nativeExecutedCopiesRead,nativeLane26Scope,type NativeExecutedCopyRead} from '../native-lane-26/client';
import type {ExecutedCopy} from "@erpx/server-legal/domain/model";
import {createExecutedCopyApplication,type ExecutedCopySource} from "@erpx/server-legal/application/executed-copies";
import { legalExecutedCopies } from '@erpx/server-collaboration/adapters/native-legal-executed-copies';
import {legalMembershipExistsSql} from '../legal-members-read-sources';
function application(db:SqlDatabase,a:WorkContext){
 const nativeExec=legalExecutedCopies(db,a.companyId,a.userId);
 const scope=nativeLane26Scope(a),reads=new Map<string,Promise<NativeExecutedCopyRead>>(),reviewsByCopy=new Map<string,Record<string,unknown>[]>();
 const readFor=(item:string)=>{let read=reads.get(item);if(!read){read=nativeExecutedCopiesRead(scope,item);reads.set(item,read);}return read;};
 return createExecutedCopyApplication({access:(id,permission)=>access(db,a,id,permission),eligible:item=>eligible(db,a,a.userId,item,"legal.approve"),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
 list:async id=>{const read=await readFor(id);return read.copies.map(copy=>{reviewsByCopy.set(copy.id,[...copy.reviews] as Record<string,unknown>[]);const {reviews:_reviews,document_author:_author,approved_author:_approver,...row}=copy;return row as unknown as ExecutedCopy&{document_name:string;document_version:number};});},
 reviews:async id=>reviewsByCopy.get(id)??[],
 member:async id=>(await readFor(id)).member,
 replay:(key,id,input)=>replayCommand(db,a,key,"legal.executed.review",id,input),
 row:async(id,itemId)=>{const copy=(await readFor(itemId)).copies.find(row=>row.id===id);return copy?copy as unknown as ExecutedCopySource:null;},
 samePerson:author=>legalEvidenceSamePerson(db,a.companyId,a.userId,author),
 approval:async id=>(await readFor(id)).approval,
 commit:async(x,item,e,authors,state,now,event,key)=>{
   return executeCommand(db,a,key,"legal.executed.review",x,item.id,item.version+1,{id:item.id,version:item.version+1,state,executedCopyId:e.id,executedCopyState:x.decision==="confirm"?"confirmed":"changes_required"},[
     ...guards(db,a,item.id,item.version,key),
     personGuard(db,a,key,a.userId,item,"legal.approve"),
     assertion(db,key,"legal-executed-reviewer",legalMembershipExistsSql,[item.id,a.userId]),
     ...authors.map((author,i)=>assertion(db,key,`legal-executed-independent-${i}`,"NOT iam_same_person(?,?,?)",[a.userId,author,a.companyId])),
     assertion(db,key,"legal-executed-current",`EXISTS(SELECT 1 FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id JOIN (${filesProjectionSources.attachments}) f ON f.id=d.attachment_source_id WHERE e.id=? AND e.version=? AND e.state!='confirmed' AND d.checksum=e.executed_hash AND p.checksum=e.approved_hash AND p.locked=true AND f.source_state='available') AND NOT EXISTS(${legalDisposedDocumentPrefix} IN (?,?)) AND ${legalLaterApprovalGuardSql}`,[e.id,e.version,e.document_id,e.approved_document_id,item.id,e.approved_cycle]),
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
