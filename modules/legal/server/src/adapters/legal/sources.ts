import {inLegalAttachmentPreservationUnit,deferLegalOwnerCheck,legalSourceWitness} from '@erpx/legal-server/adapters/purchases';
import {requireQueuedCommandAuthority} from '@erpx/identity-access-server/public/queued-command-authority';
import "server-only";
import type {PostgresDatabase} from "@erpx/platform-runtime-server/public/runtime/postgres";
import type {SqlDatabase,FileStore} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {getEmployeeRequest} from '@erpx/records-server/public/employee-requests-read';
import {getTask} from '@erpx/operations-server/public/staff-task-read';
import {readExpenseForLegal,readExpensePaymentForLegal} from "@erpx/people-payroll-server/public/legal-expense-read";
import {legalLeaveSource} from "@erpx/people-payroll-server/public/legal-evidence-read";
import {invoicePaymentOwner} from "@erpx/finance-server/public/invoice-payment-evidence";
import {attachmentsV2Readable,attachmentsV2ReadOnly} from "@erpx/files-server/public/attachments/feature";
import {attachmentContent} from "@erpx/files-server/public/attachments/service";
import {legalPreservationSource} from "@erpx/files-server/public/legal-preservation-source-facts";
import {replayCommand} from "@erpx/workflow-server/public/commands";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {access} from "@erpx/legal-server/adapters/legal/access";
import {hash} from "@erpx/legal-server/adapters/legal/service";
import {uploadDocument} from "@erpx/legal-server/adapters/legal/documents";
import {createLegalPreservationApplication,type LegalPreservationWork} from "@erpx/legal-server/application/sources";
import {legalPreservationInput} from '@erpx/legal-server/domain/sources';
import {isLegalEvidenceAttachmentType} from '@erpx/legal-server/adapters/evidence-attachment-read';
import { legalFilePreservation } from '@erpx/collaboration-server/adapters/native-legal-file-preservation';
import {legalFilesAdvisoryLockSql} from '../legal-read-tail-r9-sources';
function work(db:SqlDatabase,files:FileStore,a:WorkContext):LegalPreservationWork{return {
 lock:async()=>{await db.prepare(legalFilesAdvisoryLockSql).run();},source:id=>legalPreservationSource(db,a.companyId,id),owner:{request:id=>getEmployeeRequest(db,a,id),task:id=>getTask(db,a,id),expense:id=>readExpenseForLegal(a,id,db),leave:id=>legalLeaveSource(db,a.holdingId,id,a),expensePayment:id=>readExpensePaymentForLegal(a,id,db),invoicePayment:id=>invoicePaymentOwner(db,a,id)},content:async id=>(await attachmentContent(db,files,a,id,"original")).object,file:key=>files.get(key),replay:(key,id,input)=>replayCommand(db,a,key,"legal.document.upload",id,input),upload:(id,version,name,type,bytes,key)=>uploadDocument(db,files,a,id,version,name,type,bytes,key),
 preserve:async(id,sourceId,row,now)=>{await legalFilePreservation(db).preserve({ id, companyId: a.companyId, sourceId, objectKey: row.object_key, checksum: row.checksum_sha256, userId: a.userId, now });}
};}
export async function preserveAttachment(db:PostgresDatabase,files:FileStore,a:WorkContext,raw:unknown,key:string){const input=legalPreservationInput.parse(raw),original=await legalPreservationSource(db,a.companyId,input.attachmentId),source=original&&isLegalEvidenceAttachmentType(original.owner_type)?{attachmentId:input.attachmentId,ownerType:original.owner_type,ownerId:original.owner_id}:undefined;return createLegalPreservationApplication({readOnly:()=>attachmentsV2ReadOnly(),readable:()=>attachmentsV2Readable(),access:id=>access(db,a,id,"legal.edit"),transaction:async task=>inLegalAttachmentPreservationUnit(db,a,async tx=>{const result=await task(work(tx,files,a)),completed=await legalSourceWitness(tx,a,input.id);if(!completed)throw new ApiError(409,'LEGAL_SOURCE_CHANGED','The preserved Legal destination is unavailable.');deferLegalOwnerCheck(tx,a.companyId,async()=>{await requireQueuedCommandAuthority(tx,a,'legal.edit');if(await legalSourceWitness(tx,a,input.id)!==completed)throw new ApiError(409,'LEGAL_SOURCE_CHANGED','The completed Legal destination changed during preservation.');});return result;},source),clock:{now:()=>Date.now()},hash,error:(status,code,message)=>new ApiError(status,code,message)})(raw,key);}
