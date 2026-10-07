import { finalRootOwnerProjectionSources as filesProjectionSources } from "../../../files/public/final-root-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {legalEvidenceSamePerson} from "../../../identity_access/public/legal-evidence-identity";
import {ApiError} from "../../../runtime/public/http";
import {access,eligible,guards,personGuard} from "./access";
import {assertion,executeCommand,replayCommand} from "../../../workflow/public/commands";
import type {ExecutedCopy} from "../../domain/model";
import {createExecutedCopyApplication,type ExecutedCopySource} from "../../application/executed-copies";
function application(db:SqlDatabase,a:WorkContext){return createExecutedCopyApplication({access:(id,permission)=>access(db,a,id,permission),eligible:item=>eligible(db,a,a.userId,item,"legal.approve"),clock:{now:()=>Date.now()},identifiers:{next:()=>crypto.randomUUID()},error:(status,code,message)=>new ApiError(status,code,message),repository:{
list:async id=>(await db.prepare("SELECT e.*,d.name document_name,d.version document_version FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id WHERE e.item_id=? ORDER BY e.imported_at DESC,e.id LIMIT 100").bind(id).all<ExecutedCopy&{document_name:string;document_version:number}>()).results,
reviews:async id=>(await db.prepare("SELECT * FROM legal_executed_reviews WHERE executed_copy_id=? ORDER BY created_at,id").bind(id).all()).results,
member:async id=>Boolean(await db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?").bind(id,a.userId).first()),
replay:(key,id,input)=>replayCommand(db,a,key,"legal.executed.review",id,input),
row:(id,itemId)=>db.prepare("SELECT e.*,d.created_by document_author,p.created_by approved_author FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id WHERE e.id=? AND e.item_id=?").bind(id,itemId).first<ExecutedCopySource>(),
samePerson:author=>legalEvidenceSamePerson(db,a.companyId,a.userId,author),
approval:id=>db.prepare("SELECT cycle FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1").bind(id).first<{cycle:number}>(),
commit:async(x,item,e,authors,state,now,event,key)=>{
  return executeCommand(db,a,key,"legal.executed.review",x,item.id,item.version+1,{id:item.id,version:item.version+1,state,executedCopyId:e.id,executedCopyState:x.decision==="confirm"?"confirmed":"changes_required"},[
    ...guards(db,a,item.id,item.version,key),
    personGuard(db,a,key,a.userId,item,"legal.approve"),
    assertion(db,key,"legal-executed-reviewer","EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[item.id,a.userId]),
    ...authors.map((author,i)=>assertion(db,key,`legal-executed-independent-${i}`,"NOT iam_same_person(?,?,?)",[a.userId,author,a.companyId])),
    assertion(db,key,"legal-executed-current",`EXISTS(SELECT 1 FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id JOIN (${filesProjectionSources.attachments}) f ON f.id=d.attachment_source_id WHERE e.id=? AND e.version=? AND e.state!='confirmed' AND d.checksum=e.executed_hash AND p.checksum=e.approved_hash AND p.locked=true AND f.source_state='available') AND NOT EXISTS(SELECT 1 FROM legal_disposals WHERE document_id IN (?,?)) AND NOT EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle>?)`,[e.id,e.version,e.document_id,e.approved_document_id,item.id,e.approved_cycle]),
    db.prepare("INSERT INTO legal_executed_reviews(id,executed_copy_id,item_version,decision,checks,reason,reviewer_id,created_at) VALUES(?,?,?,?,?::jsonb,?,?,?)").bind(key,e.id,item.version,x.decision,JSON.stringify(x.checks),x.reason,a.userId,now),
    db.prepare("UPDATE legal_executed_copies SET state=?,reviewed_by=?,reviewed_at=?,version=version+1 WHERE id=?").bind(x.decision==="confirm"?"confirmed":"changes_required",x.decision==="confirm"?a.userId:null,x.decision==="confirm"?now:null,e.id),
    db.prepare("UPDATE legal_items SET state=?,version=version+1,updated_at=? WHERE id=?").bind(state,now,item.id),
    db.prepare("INSERT INTO legal_events(id,item_id,action,actor_id,note,version,created_at) VALUES(?,?,?,?,?,?,?)").bind(event.id,item.id,`executed.${x.decision}`,a.userId,`${e.id} · ${x.reason}`,item.version+1,event.now),
  ]);
}
}});}
export function executedCopies(db:SqlDatabase,a:WorkContext,id:string){return application(db,a).executedCopies(id);}
export function reviewExecutedCopy(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){return application(db,a).reviewExecutedCopy(raw,key);}
