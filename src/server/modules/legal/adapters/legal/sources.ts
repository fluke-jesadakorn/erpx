import "server-only";
import type {PostgresDatabase} from "../../../runtime/public/runtime/postgres";
import type {SqlDatabase,FileStore} from "../../../runtime/public/runtime/types";
import type {WorkContext} from "../../../identity_access/public/authorization";
import {getEmployeeRequest} from "../../../records/public/employee-requests";
import {getTask} from "../../../operations/public/staff-tasks";
import {readExpenseForLegal,readExpensePaymentForLegal} from "../../../people_payroll/public/legal-expense-read";
import {legalLeaveSource} from "../../../people_payroll/public/legal-evidence-read";
import {invoicePaymentOwner} from "../../../finance/public/invoice-payment-evidence";
import {attachmentsV2Readable,attachmentsV2ReadOnly} from "../../../files/public/attachments/feature";
import {attachmentContent} from "../../../files/public/attachments/service";
import {legalPreservationSource} from "../../../files/public/legal-preservation-source-facts";
import {replayCommand} from "../../../workflow/public/commands";
import {ApiError} from "../../../runtime/public/http";
import {access} from "./access";
import {hash} from "./service";
import {uploadDocument} from "./documents";
import {createLegalPreservationApplication,type LegalPreservationWork} from "../../application/sources";
function work(db:SqlDatabase,files:FileStore,a:WorkContext):LegalPreservationWork{return {
 lock:async()=>{await db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run();},source:id=>legalPreservationSource(db,a.companyId,id),owner:{request:id=>getEmployeeRequest(db,a,id),task:id=>getTask(db,a,id),expense:id=>readExpenseForLegal(a,id,db),leave:id=>legalLeaveSource(db,a.holdingId,id,a),expensePayment:id=>readExpensePaymentForLegal(a,id,db),invoicePayment:id=>invoicePaymentOwner(db,a,id)},content:async id=>(await attachmentContent(db,files,a,id,"original")).object,file:key=>files.get(key),replay:(key,id,input)=>replayCommand(db,a,key,"legal.document.upload",id,input),upload:(id,version,name,type,bytes,key)=>uploadDocument(db,files,a,id,version,name,type,bytes,key),
 preserve:async(id,sourceId,row,now)=>{await db.prepare("INSERT INTO legal_file_preservations(item_id,source_type,source_id,object_key,checksum,created_by,created_at) VALUES(?,'attachment',?,?,?,?,?) ON CONFLICT DO NOTHING").bind(id,sourceId,row.object_key,row.checksum_sha256,a.userId,now).run();}
};}
export function preserveAttachment(db:PostgresDatabase,files:FileStore,a:WorkContext,raw:unknown,key:string){return createLegalPreservationApplication({readOnly:()=>attachmentsV2ReadOnly(),readable:()=>attachmentsV2Readable(),access:id=>access(db,a,id,"legal.edit"),transaction:task=>db.transaction(tx=>task(work(tx,files,a))),clock:{now:()=>Date.now()},hash,error:(status,code,message)=>new ApiError(status,code,message)})(raw,key);}
