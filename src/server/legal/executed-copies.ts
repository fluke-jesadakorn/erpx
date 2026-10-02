import "server-only";
import { z } from "zod";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import type { ExecutedCopy } from "@/lib/legal/model";
import { access, eligible, event, fail, guards, personGuard } from "./access";
import { assertion, executeCommand, replayCommand } from "../commands";

export async function executedCopies(db:SqlDatabase,a:WorkContext,id:string){
  await access(db,a,id);
  const rows=(await db.prepare("SELECT e.*,d.name document_name,d.version document_version FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id WHERE e.item_id=? ORDER BY e.imported_at DESC,e.id LIMIT 100").bind(id).all<ExecutedCopy&{document_name:string;document_version:number}>()).results;
  return Promise.all(rows.map(async e=>({...e,metadata:typeof e.metadata==="string"?JSON.parse(e.metadata):e.metadata,reviews:(await db.prepare("SELECT * FROM legal_executed_reviews WHERE executed_copy_id=? ORDER BY created_at,id").bind(e.id).all()).results.map(r=>({...r,checks:typeof r.checks==="string"?JSON.parse(r.checks):r.checks}))})));
}
const checksSchema=z.object({contentMatches:z.boolean(),pagesComplete:z.boolean(),partiesVerified:z.boolean(),signaturesComplete:z.boolean(),authorityVerified:z.boolean()}).strict();
export async function reviewExecutedCopy(db:SqlDatabase,a:WorkContext,raw:unknown,key:string){
  const x=z.object({action:z.literal("executed.review"),id:z.string().min(1),expectedVersion:z.number().int().positive(),executedCopyId:z.string().min(1),decision:z.enum(["confirm","changes_required"]),checks:checksSchema,reason:z.string().trim().min(10).max(4000)}).strict().parse(raw);
  const item=await access(db,a,x.id,"legal.approve");
  await eligible(db,a,a.userId,item,"legal.approve");
  if(!(await db.prepare("SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?").bind(item.id,a.userId).first()))fail("The reviewer needs explicit contract membership.","LEGAL_REVIEWER_UNAVAILABLE",403);
  const replay=await replayCommand(db,a,key,"legal.executed.review",item.id,x);
  if(replay)return replay;
  if(item.version!==x.expectedVersion)fail("The contract changed. Refresh first.","LEGAL_VERSION_CONFLICT",409);
  const e=await db.prepare("SELECT e.*,d.created_by document_author,p.created_by approved_author FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id WHERE e.id=? AND e.item_id=?").bind(x.executedCopyId,item.id).first<ExecutedCopy&{document_author:string;approved_author:string}>();
  if(!e)fail("Executed copy unavailable.","LEGAL_NOT_FOUND",404);
  if(e.state==="confirmed"||item.kind!=="contract"||!["approved","executed","active"].includes(item.state))fail("Import a new revision to correct a confirmed copy.","LEGAL_EXECUTED_STATE",409);
  const authors=[e.imported_by,e.document_author,e.approved_author,item.created_by];
  for(const author of authors)if(await db.prepare("SELECT 1 WHERE iam_same_person(?,?,?)").bind(a.userId,author,a.companyId).first())fail("An independent reviewer must inspect the signed original.","LEGAL_INDEPENDENT_REVIEW",403);
  if(x.decision==="confirm"&&!Object.values(x.checks).every(Boolean))fail("Confirm content, complete pages, parties, signatures and authority before accepting an executed copy.","LEGAL_EXECUTED_INCOMPLETE",409);
  const approval=await db.prepare("SELECT cycle FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1").bind(item.id).first<{cycle:number}>();
  if(approval?.cycle!==e.approved_cycle)fail("The approved version changed. Import against the current approval.","LEGAL_APPROVAL_STALE",409);
  const now=Date.now(),state=x.decision==="confirm"&&item.state==="approved"?"executed":item.state;
  return executeCommand(db,a,key,"legal.executed.review",x,item.id,item.version+1,{id:item.id,version:item.version+1,state,executedCopyId:e.id,executedCopyState:x.decision==="confirm"?"confirmed":"changes_required"},[
    ...guards(db,a,item.id,item.version,key),
    personGuard(db,a,key,a.userId,item,"legal.approve"),
    assertion(db,key,"legal-executed-reviewer","EXISTS(SELECT 1 FROM legal_members WHERE item_id=? AND user_id=?)",[item.id,a.userId]),
    ...authors.map((author,i)=>assertion(db,key,`legal-executed-independent-${i}`,"NOT iam_same_person(?,?,?)",[a.userId,author,a.companyId])),
    assertion(db,key,"legal-executed-current","EXISTS(SELECT 1 FROM legal_executed_copies e JOIN legal_documents d ON d.id=e.document_id JOIN legal_documents p ON p.id=e.approved_document_id JOIN attachments f ON f.id=d.attachment_source_id WHERE e.id=? AND e.version=? AND e.state!='confirmed' AND d.checksum=e.executed_hash AND p.checksum=e.approved_hash AND p.locked=true AND f.source_state='available') AND NOT EXISTS(SELECT 1 FROM legal_disposals WHERE document_id IN (?,?)) AND NOT EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle>?)",[e.id,e.version,e.document_id,e.approved_document_id,item.id,e.approved_cycle]),
    db.prepare("INSERT INTO legal_executed_reviews(id,executed_copy_id,item_version,decision,checks,reason,reviewer_id,created_at) VALUES(?,?,?,?,?::jsonb,?,?,?)").bind(key,e.id,item.version,x.decision,JSON.stringify(x.checks),x.reason,a.userId,now),
    db.prepare("UPDATE legal_executed_copies SET state=?,reviewed_by=?,reviewed_at=?,version=version+1 WHERE id=?").bind(x.decision==="confirm"?"confirmed":"changes_required",x.decision==="confirm"?a.userId:null,x.decision==="confirm"?now:null,e.id),
    db.prepare("UPDATE legal_items SET state=?,version=version+1,updated_at=? WHERE id=?").bind(state,now,item.id),
    event(db,a,item.id,`executed.${x.decision}`,`${e.id} · ${x.reason}`,item.version+1),
  ]);
}
