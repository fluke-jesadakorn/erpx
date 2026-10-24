import {inLegalAttachmentPreservationUnit,deferLegalOwnerCheck,legalSourceWitness} from '@erpx/server-legal/adapters/purchases';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import "server-only";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase,FileStore} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {getEmployeeRequest} from '@erpx/server-records/public/employee-requests-read';
import {getTask} from '@erpx/server-operations/public/staff-task-read';
import {readExpenseForLegal,readExpensePaymentForLegal} from "@erpx/server-people_payroll/public/legal-expense-read";
import {legalLeaveSource} from "@erpx/server-people_payroll/public/legal-evidence-read";
import {invoicePaymentOwner} from "@erpx/server-finance/public/invoice-payment-evidence";
import {attachmentsV2Readable,attachmentsV2ReadOnly} from "@erpx/server-files/public/attachments/feature";
import {attachmentContent} from "@erpx/server-files/public/attachments/service";
import {legalPreservationSource} from "@erpx/server-files/public/legal-preservation-source-facts";
import {replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {access} from "@erpx/server-legal/adapters/legal/access";
import {hash} from "@erpx/server-legal/adapters/legal/service";
import {uploadDocument} from "@erpx/server-legal/adapters/legal/documents";
import {createLegalPreservationApplication,type LegalPreservationWork} from "@erpx/server-legal/application/sources";
import {legalPreservationInput} from '@erpx/server-legal/domain/sources';
import {isLegalEvidenceAttachmentType} from '@erpx/server-legal/adapters/evidence-attachment-read';
function work(db:SqlDatabase,files:FileStore,a:WorkContext):LegalPreservationWork{return {
 lock:async()=>{await db.prepare("SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))").run();},source:id=>legalPreservationSource(db,a.companyId,id),owner:{request:id=>getEmployeeRequest(db,a,id),task:id=>getTask(db,a,id),expense:id=>readExpenseForLegal(a,id,db),leave:id=>legalLeaveSource(db,a.holdingId,id,a),expensePayment:id=>readExpensePaymentForLegal(a,id,db),invoicePayment:id=>invoicePaymentOwner(db,a,id)},content:async id=>(await attachmentContent(db,files,a,id,"original")).object,file:key=>files.get(key),replay:(key,id,input)=>replayCommand(db,a,key,"legal.document.upload",id,input),upload:(id,version,name,type,bytes,key)=>uploadDocument(db,files,a,id,version,name,type,bytes,key),
 preserve:async(id,sourceId,row,now)=>{await db.prepare("INSERT INTO legal_file_preservations(item_id,source_type,source_id,object_key,checksum,created_by,created_at) VALUES(?,'attachment',?,?,?,?,?) ON CONFLICT DO NOTHING").bind(id,sourceId,row.object_key,row.checksum_sha256,a.userId,now).run();}
};}
export async function preserveAttachment(db:PostgresDatabase,files:FileStore,a:WorkContext,raw:unknown,key:string){const input=legalPreservationInput.parse(raw),original=await legalPreservationSource(db,a.companyId,input.attachmentId),source=original&&isLegalEvidenceAttachmentType(original.owner_type)?{attachmentId:input.attachmentId,ownerType:original.owner_type,ownerId:original.owner_id}:undefined;return createLegalPreservationApplication({readOnly:()=>attachmentsV2ReadOnly(),readable:()=>attachmentsV2Readable(),access:id=>access(db,a,id,"legal.edit"),transaction:async task=>inLegalAttachmentPreservationUnit(db,a,async tx=>{const result=await task(work(tx,files,a)),completed=await legalSourceWitness(tx,a,input.id);if(!completed)throw new ApiError(409,'LEGAL_SOURCE_CHANGED','The preserved Legal destination is unavailable.');deferLegalOwnerCheck(tx,a.companyId,async()=>{await requireQueuedCommandAuthority(tx,a,'legal.edit');if(await legalSourceWitness(tx,a,input.id)!==completed)throw new ApiError(409,'LEGAL_SOURCE_CHANGED','The completed Legal destination changed during preservation.');});return result;},source),clock:{now:()=>Date.now()},hash,error:(status,code,message)=>new ApiError(status,code,message)})(raw,key);}
