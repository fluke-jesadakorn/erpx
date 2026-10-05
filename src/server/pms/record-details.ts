import 'server-only';
import {z} from 'zod';
import {loadPms} from '../pms';
import type {WorkContext} from '../authorization';
import {holdingResources} from '../runtime';
import {requirePermission} from '../iam-permissions';
import {ApiError} from '../http';
import {hasPermission} from '@/lib/permissions';
import type {PmsRecord} from '@/lib/pms-model';
/** Console finance queues keep their original company-level visibility. */
export async function pmsRefundRecord(actor:WorkContext,kind:'booking'|'counter',id:string){
 requirePermission(actor,'pms.read');
 if(!hasPermission(actor,'finance.approve')||actor.branchId||actor.departmentId)throw new ApiError(403,'PMS_REFUND_REVIEW_FORBIDDEN','Finance refund review access is required.');
 const {write}=await holdingResources(actor.holdingId);
 const sql=kind==='booking'?'SELECT r.id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.created_at,r.last_error,r.reason FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE r.id=? AND p.company_id=?':'SELECT id,property_id,folio_id,amount_minor,refund_amount_minor,refund_state,last_error,refund_reason,payer_email FROM pms_stripe_payments WHERE id=? AND company_id=? AND refund_state IS NOT NULL';
 const row=await write.prepare(sql).bind(id,actor.companyId).first<Record<string,string|number|null>>();
 if(!row)throw new ApiError(404,'REFUND_NOT_FOUND','Refund not found in this company.');
 await loadPms(write,actor,String(row.property_id));
 return {row:{...row,name:kind==='booking'?'Online booking refund':'Counter payment refund',currency:'THB'},refundKind:kind};
}
export async function pmsSnapshotEntry(actor:WorkContext,id:string,documentId:string){
 requirePermission(actor,'pms.read');
 if(!hasPermission(actor,'pms.cashier')&&!hasPermission(actor,'pms.reports'))throw new ApiError(403,'PMS_DOCUMENT_FORBIDDEN','Document access is required.');
 const {write}=await holdingResources(actor.holdingId);
 const doc=await write.prepare('SELECT property_id,snapshot_json FROM pms_documents WHERE id=?').bind(documentId).first<{property_id:string;snapshot_json:string}>();
 if(!doc)throw new ApiError(404,'DOCUMENT_NOT_FOUND','Property document not found.');
 await loadPms(write,actor,doc.property_id);
 const snap=JSON.parse(doc.snapshot_json) as {entries:PmsRecord<'entry'>[];property:{currency:string}};
 const entry=snap.entries.find(row=>row.id===id);
 if(!entry)throw new ApiError(404,'ENTRY_NOT_FOUND','Entry not found in this retained document.');
 const {date,kind,description,reference,net,service,tax,total,method,originalId}=entry.data;
 return {row:{id,name:description,description,version:entry.version,currency:snap.property.currency,date,kind,reference,net,service,tax,total,method,originalId},related:[{kind:'pms_document' as const,id:documentId}]};
}
export async function pmsControlSources(actor:WorkContext,propertyId:string,key:string,page:number){
 requirePermission(actor,'pms.read');requirePermission(actor,'pms.reports');
 const control=z.enum(['receivable','advance','deposit']).parse(key),{write}=await holdingResources(actor.holdingId),snapshot=await loadPms(write,actor,propertyId),account=snapshot.property.config.accounts[control];
 const joins=`FROM finance_journal_lines l JOIN (SELECT property_id,journal_id FROM pms_postings UNION SELECT property_id,opening_journal_id journal_id FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL) p ON p.journal_id=l.journal_id JOIN finance_journals j ON j.id=l.journal_id WHERE p.property_id=? AND j.status='posted' AND l.account_id=?`;
 const rows=(await write.prepare(`SELECT l.*,j.id journal_id,j.journal_number,j.entry_date,j.status,j.currency ${joins} ORDER BY j.entry_date,j.id,l.ordinal LIMIT 31 OFFSET ?`).bind(propertyId,account,(page-1)*30).all()).results;
 const amount=await write.prepare(`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) balance ${joins}`).bind(propertyId,account).first<{balance:number}>();
 return {row:{name:`${snapshot.property.name} · ${control}`,currency:snapshot.property.config.currency,amount_minor:Number(amount?.balance??0)*(control==='receivable'?1:-1)},rows,page,hasNext:rows.length>30};
}
