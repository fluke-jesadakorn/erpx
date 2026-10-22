import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {taxDocumentId,type Invoice,type Document,type TaxDocumentCommand,type TaxDocumentResult} from "@erpx/server-finance/domain/hospitality-tax-documents";
type RecordCommand=Extract<TaxDocumentCommand,{action:'record'}>;type DecisionCommand=Exclude<TaxDocumentCommand,RecordCommand>;
export interface TaxDocumentRepository {
 invoice(actor:WorkContext,id:string):Promise<Invoice|null>;
 document(invoiceId:string,id:string):Promise<Document|null>;
 documents(companyId:string,invoiceId:string):Promise<Document[]>;
 replay(actor:WorkContext,key:string,operation:string,recordId:string,command:TaxDocumentCommand):Promise<TaxDocumentResult|null>;
 commitRecord(facts:{actor:WorkContext;key:string;operation:string;command:RecordCommand;recordId:string;invoice:Invoice;documentId:string;now:number;result:TaxDocumentResult}):Promise<TaxDocumentResult>;
 commitDecision(facts:{actor:WorkContext;key:string;operation:string;command:DecisionCommand;recordId:string;invoice:Invoice;doc:Document;next:number;now:number;state:string;result:TaxDocumentResult}):Promise<TaxDocumentResult>;
}
export interface TaxDocumentPorts {repository:TaxDocumentRepository;ready(actor:WorkContext):Promise<void>;assertEnabled(actor:WorkContext,source:string):Promise<void>;hasPermission(actor:WorkContext,permission:string):boolean;now():number;id():string;error(status:number,code:string,message:string):Error;}
export function createHospitalityTaxDocuments(ports:TaxDocumentPorts){
 async function invoiceFor(actor:WorkContext,invoiceId:string):Promise<Invoice>{
 const row=await ports.repository.invoice(actor,invoiceId);
 if (!row || !['pos','pms'].includes(row.source_type) || !row.branch_id || (actor.branchId && actor.branchId!==row.branch_id) || actor.departmentId || actor.beta)
 throw ports.error(404,'TAX_DOCUMENT_INVOICE_NOT_FOUND','Hotel or restaurant invoice not found in this workspace.');
 return row;
 }
 function requireCashier(actor:WorkContext,source:string){if(!ports.hasPermission(actor,source==='pos'?'pos.sell':'pms.cashier'))throw ports.error(403,'PERMISSION_REQUIRED','Your selected role cannot record this document reference.');}
 function requireReviewer(actor:WorkContext){if(!ports.hasPermission(actor,'finance.approve'))throw ports.error(403,'PERMISSION_REQUIRED','A Finance approver must review this reference.');}
 async function documentFor(invoiceId:string,documentId:string){const row=await ports.repository.document(invoiceId,documentId);if(!row)throw ports.error(404,'TAX_DOCUMENT_NOT_FOUND','Document reference not found.');return row;}
async function taxDocumentView(actor:WorkContext, invoiceId:string) {
  await ports.ready(actor);
  const invoice=await invoiceFor(actor,taxDocumentId.parse(invoiceId));
  if (!ports.hasPermission(actor,'finance.read') && !ports.hasPermission(actor,invoice.source_type==='pos'?'pos.read':'pms.cashier'))
    throw ports.error(403,'PERMISSION_REQUIRED','Your selected role cannot view this document reference.');
  const documents=await ports.repository.documents(actor.companyId,invoice.id);
  return { invoice:{id:invoice.id,sourceType:invoice.source_type,currency:invoice.currency,totalMinor:Number(invoice.total_minor),taxMinor:Number(invoice.tax_minor),status:invoice.status}, documents, canRecord:ports.hasPermission(actor,invoice.source_type==='pos'?'pos.sell':'pms.cashier'), canReview:ports.hasPermission(actor,'finance.approve'), userId:actor.userId };
}
/** Front records its own operational source; independent Finance review stays with Finance. */
async function frontTaxDocumentView(actor:WorkContext, invoiceId:string, app:'pos'|'pms') {
  const view=await taxDocumentView(actor,invoiceId);
  if(view.invoice.sourceType!==app)throw ports.error(404,'TAX_DOCUMENT_INVOICE_NOT_FOUND','Invoice not found in this Front app.');
  return {...view,canReview:false};
}
async function frontTaxDocumentCommand(actor:WorkContext, command:TaxDocumentCommand, key:string, app:'pos'|'pms') {
  if(command.action!=='record')throw ports.error(403,'FRONT_COMMAND_FORBIDDEN','Independent document review belongs in Finance.');
  await frontTaxDocumentView(actor,command.invoiceId,app);
  return taxDocumentCommand(actor,command,key);
}
async function taxDocumentCommand(actor:WorkContext, command:TaxDocumentCommand, key:string) {
  await ports.ready(actor);
  const operation=`hospitality.tax_document.${command.action}`;
  const recordId=command.action==='record'?command.invoiceId:command.documentId;
  const replay=await ports.repository.replay(actor,key,operation,recordId,command);
  if (replay) return replay;
  const invoice=await invoiceFor(actor,command.invoiceId);
  await ports.assertEnabled(actor,invoice.source_type);
  if (command.action==='record') {
    requireCashier(actor,invoice.source_type);
    if (!['posted','partially_paid','paid'].includes(invoice.status)) throw ports.error(409,'INVOICE_NOT_POSTED','Issue the operational invoice before recording an external tax document.');
    const now=ports.now(),documentId=ports.id();
    const result={id:documentId,state:'recorded',version:1};
    return ports.repository.commitRecord({actor,key,operation,command,recordId,invoice,documentId,now,result});
  }
  requireReviewer(actor);
  const doc=await documentFor(invoice.id,command.documentId);
  if (doc.company_id!==actor.companyId) throw ports.error(404,'TAX_DOCUMENT_NOT_FOUND','Document reference not found.');
  if (doc.version!==command.expectedVersion) throw ports.error(409,'STALE_DOCUMENT','Document reference changed. Reload and review again.');
  if (command.action==='verify') {
    if (doc.state!=='recorded' || doc.recorded_by===actor.userId) throw ports.error(409,'INDEPENDENT_REVIEW_REQUIRED','A different Finance approver must verify a recorded reference.');
    if (doc.currency!==invoice.currency || Number(doc.total_minor)!==Number(invoice.total_minor) || Number(doc.tax_minor)!==Number(invoice.tax_minor) || doc.document_total_minor===null || doc.document_tax_minor===null || Number(doc.document_total_minor)!==Number(invoice.total_minor) || Number(doc.document_tax_minor)!==Number(invoice.tax_minor) || invoice.status==='voided')
      throw ports.error(409,'INVOICE_CHANGED','Invoice amount, tax, currency, or status changed. Review the source before verifying.');
  } else if (doc.state==='voided') throw ports.error(409,'ALREADY_VOIDED','This reference was already voided.');
  const next=doc.version+1,now=ports.now(),state=command.action==='verify'?'verified':'voided';
  const result={id:doc.id,state,version:next};
  return ports.repository.commitDecision({actor,key,operation,command,recordId,invoice,doc,next,now,state,result});
}

return {taxDocumentView,frontTaxDocumentView,frontTaxDocumentCommand,taxDocumentCommand};}
