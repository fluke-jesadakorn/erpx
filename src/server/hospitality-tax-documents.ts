import 'server-only';
import { randomUUID } from 'node:crypto';
import { z } from 'zod';
import { hasPermission } from '@/lib/permissions';
import type { WorkContext } from './authorization';
import { executeCommand, replayCommand, assertion } from './commands';
import { assertProductionModuleEnabled } from './hospitality-rollout';
import { ApiError } from './http';
import { holdingResources } from './runtime';
import type { SqlDatabase } from './runtime/types';

const id = z.string().uuid();
export const taxDocumentCommandSchema = z.discriminatedUnion('action', [
  z.object({ action: z.literal('record'), invoiceId: id, issuer: z.string().trim().min(2).max(160), externalReference: z.string().trim().min(2).max(100), issuedOn: z.iso.date(), reason: z.string().trim().min(3).max(500) }),
  z.object({ action: z.literal('verify'), invoiceId: id, documentId: id, expectedVersion: z.number().int().positive() }),
  z.object({ action: z.literal('void'), invoiceId: id, documentId: id, expectedVersion: z.number().int().positive(), reason: z.string().trim().min(3).max(500) }),
]);
export type TaxDocumentCommand = z.infer<typeof taxDocumentCommandSchema>;
type Invoice = { id:string; company_id:string; source_type:string; source_id:string; currency:string; total_minor:number; tax_minor:number; status:string; branch_id:string | null };
type Document = { id:string; company_id:string; invoice_id:string; source_type:string; issuer:string; external_reference:string; issued_on:string; currency:string; total_minor:number; tax_minor:number; state:string; version:number; recorded_by:string; verified_by:string|null; voided_by:string|null; reason:string; created_at:number; updated_at:number };

async function invoiceFor(db:SqlDatabase, actor:WorkContext, invoiceId:string):Promise<Invoice> {
  const row=await db.prepare(`SELECT i.id,i.company_id,i.source_type,i.source_id,i.currency,i.total_minor,i.tax_minor,i.status,
    CASE WHEN i.source_type='pos' THEN s.branch_id WHEN i.source_type='pms' THEN p.branch_id ELSE NULL END branch_id
    FROM finance_invoices i
    LEFT JOIN pos_sales s ON i.source_type='pos' AND s.id=i.source_id AND s.invoice_id=i.id AND s.company_id=i.company_id AND s.state='completed'
    LEFT JOIN pms_documents d ON i.source_type='pms' AND d.finance_id=i.id AND d.kind='invoice' AND d.folio_id=i.source_id
    LEFT JOIN pms_properties p ON p.id=d.property_id AND p.company_id=i.company_id
    WHERE i.id=? AND i.company_id=?`).bind(invoiceId,actor.companyId).first<Invoice>();
  if (!row || !['pos','pms'].includes(row.source_type) || !row.branch_id || (actor.branchId && actor.branchId!==row.branch_id) || actor.departmentId || actor.beta)
    throw new ApiError(404,'TAX_DOCUMENT_INVOICE_NOT_FOUND','Hotel or restaurant invoice not found in this workspace.');
  return row;
}
function requireCashier(actor:WorkContext, source:string) {
  if (!hasPermission(actor,source==='pos'?'pos.sell':'pms.cashier'))
    throw new ApiError(403,'PERMISSION_REQUIRED','Your selected role cannot record this document reference.');
}
function requireReviewer(actor:WorkContext) {
  if (!hasPermission(actor,'finance.approve'))
    throw new ApiError(403,'PERMISSION_REQUIRED','A Finance approver must review this reference.');
}
async function documentFor(db:SqlDatabase, invoiceId:string, documentId:string) {
  const row=await db.prepare('SELECT * FROM hospitality_tax_documents WHERE id=? AND invoice_id=?').bind(documentId,invoiceId).first<Document>();
  if (!row) throw new ApiError(404,'TAX_DOCUMENT_NOT_FOUND','Document reference not found.');
  return row;
}
export async function taxDocumentView(actor:WorkContext, invoiceId:string) {
  const {write:db}=await holdingResources(actor.holdingId);
  const invoice=await invoiceFor(db,actor,id.parse(invoiceId));
  if (!hasPermission(actor,'finance.read') && !hasPermission(actor,invoice.source_type==='pos'?'pos.read':'pms.cashier'))
    throw new ApiError(403,'PERMISSION_REQUIRED','Your selected role cannot view this document reference.');
  const documents=(await db.prepare('SELECT * FROM hospitality_tax_documents WHERE company_id=? AND invoice_id=? ORDER BY created_at DESC,id DESC').bind(actor.companyId,invoice.id).all<Document>()).results;
  return { invoice:{id:invoice.id,sourceType:invoice.source_type,currency:invoice.currency,totalMinor:Number(invoice.total_minor),taxMinor:Number(invoice.tax_minor),status:invoice.status}, documents, canRecord:hasPermission(actor,invoice.source_type==='pos'?'pos.sell':'pms.cashier'), canReview:hasPermission(actor,'finance.approve'), userId:actor.userId };
}
export async function taxDocumentCommand(actor:WorkContext, command:TaxDocumentCommand, key:string) {
  const {write:db}=await holdingResources(actor.holdingId);
  const operation=`hospitality.tax_document.${command.action}`;
  const recordId=command.action==='record'?command.invoiceId:command.documentId;
  const replay=await replayCommand(db,actor,key,operation,recordId,command);
  if (replay) return replay;
  const invoice=await invoiceFor(db,actor,command.invoiceId);
  await assertProductionModuleEnabled(db,actor.companyId,invoice.source_type);
  if (command.action==='record') {
    requireCashier(actor,invoice.source_type);
    if (!['posted','partially_paid','paid'].includes(invoice.status)) throw new ApiError(409,'INVOICE_NOT_POSTED','Issue the operational invoice before recording an external tax document.');
    const now=Date.now(),documentId=randomUUID();
    const result={id:documentId,state:'recorded',version:1};
    return executeCommand(db,actor,key,operation,command,recordId,1,result,[
      assertion(db,key,'invoice-current',`EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND source_type=? AND currency=? AND total_minor=? AND tax_minor=? AND status IN ('posted','partially_paid','paid'))`,[invoice.id,actor.companyId,invoice.source_type,invoice.currency,invoice.total_minor,invoice.tax_minor]),
      db.prepare(`INSERT INTO hospitality_tax_documents(id,company_id,invoice_id,source_type,issuer,external_reference,issued_on,currency,total_minor,tax_minor,state,version,recorded_by,reason,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,?,?,?,'recorded',1,?,?,?,?)`).bind(documentId,actor.companyId,invoice.id,invoice.source_type,command.issuer,command.externalReference,command.issuedOn,invoice.currency,invoice.total_minor,invoice.tax_minor,actor.userId,command.reason,now,now),
    ]);
  }
  requireReviewer(actor);
  const doc=await documentFor(db,invoice.id,command.documentId);
  if (doc.company_id!==actor.companyId) throw new ApiError(404,'TAX_DOCUMENT_NOT_FOUND','Document reference not found.');
  if (doc.version!==command.expectedVersion) throw new ApiError(409,'STALE_DOCUMENT','Document reference changed. Reload and review again.');
  if (command.action==='verify') {
    if (doc.state!=='recorded' || doc.recorded_by===actor.userId) throw new ApiError(409,'INDEPENDENT_REVIEW_REQUIRED','A different Finance approver must verify a recorded reference.');
    if (doc.currency!==invoice.currency || Number(doc.total_minor)!==Number(invoice.total_minor) || Number(doc.tax_minor)!==Number(invoice.tax_minor) || invoice.status==='voided')
      throw new ApiError(409,'INVOICE_CHANGED','Invoice amount, tax, currency, or status changed. Review the source before verifying.');
  } else if (doc.state==='voided') throw new ApiError(409,'ALREADY_VOIDED','This reference was already voided.');
  const next=doc.version+1,now=Date.now(),state=command.action==='verify'?'verified':'voided';
  const result={id:doc.id,state,version:next};
  return executeCommand(db,actor,key,operation,command,recordId,next,result,[
    assertion(db,key,'invoice-current',`EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND source_type=? AND currency=? AND total_minor=? AND tax_minor=? AND status IN ('posted','partially_paid','paid'))`,[invoice.id,actor.companyId,invoice.source_type,invoice.currency,invoice.total_minor,invoice.tax_minor]),
    assertion(db,key,'document-current',`EXISTS(SELECT 1 FROM hospitality_tax_documents WHERE id=? AND company_id=? AND invoice_id=? AND version=? AND state=? AND currency=? AND total_minor=? AND tax_minor=?)`,[doc.id,actor.companyId,invoice.id,doc.version,doc.state,invoice.currency,invoice.total_minor,invoice.tax_minor]),
    db.prepare(`UPDATE hospitality_tax_documents SET state=?,version=?,${command.action==='verify'?'verified_by':'voided_by'}=?,reason=?,updated_at=? WHERE id=? AND company_id=? AND version=?`).bind(state,next,actor.userId,command.action==='void'?command.reason:doc.reason,now,doc.id,actor.companyId,doc.version),
  ]);
}
