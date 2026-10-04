import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import { samePerson } from "@/lib/iam";
import { renderThaiDocument, type ThaiDocumentSnapshot } from "@/lib/finance-thai-document";
import { thaiFinanceTaxPack } from "@/lib/thai-finance-tax-pack";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import type { SqlDatabase } from "./runtime/types";
import { currentTaxSignerGuards, requireCompanyTaxAuthority, reviewedTaxIdentity, taxIdentityGuards, taxSourceDigest, type TaxIdentity } from "./finance-tax-identities";
import { currentInvoiceLines } from "./finance-invoice-lines";

type IssuedDocument = { id:string;company_id:string;document_kind:ThaiDocumentSnapshot["kind"];document_number:string;issue_date:string;payment_id:string|null;invoice_id:string|null;assessment_id:string|null;previous_document_id:string|null;edition:number;source_fingerprint:string;snapshot_json:string;export_html:string;export_sha256:string;created_at:number };
export const thaiDocumentChangeSchema=z.object({payerIdentityId:z.string().min(1),payeeIdentityId:z.string().min(1),issueDate:z.iso.date(),reason:z.string().trim().min(10).max(2000)});
export const thaiDocumentDecisionSchema=z.object({expectedVersion:z.number().int().positive(),action:z.enum(["approve","reject","cancel"]),reason:z.string().trim().min(10).max(2000)});
export const thaiInvoiceIssueSchema=z.object({expectedVersion:z.number().int().positive(),payerIdentityId:z.string().min(1),payeeIdentityId:z.string().min(1),reason:z.string().trim().min(10).max(2000)});

export async function prepareThaiDocumentIssue(db:SqlDatabase,actor:WorkContext,input:{snapshot:Omit<ThaiDocumentSnapshot,"documentNumber">;payer:TaxIdentity;payee:TaxIdentity;paymentId?:string;invoiceId?:string;assessmentId?:string;previous?:IssuedDocument;commandId:string}){
  const {snapshot,payer,payee,commandId}=input,year=Number(snapshot.issueDate.slice(0,4));
  const last=Number(await db.prepare("SELECT last_number FROM finance_thai_document_sequences WHERE company_id=? AND document_kind=? AND fiscal_year=?").bind(actor.companyId,snapshot.kind,year).first("last_number")??0),sequence=last+1;
  const number=`${snapshot.kind==="withholding_certificate"?"WHT":"VAT"}-${year}-${String(sequence).padStart(6,"0")}`,id=`thai-document-${commandId}`;
  const complete:ThaiDocumentSnapshot={...snapshot,documentNumber:number},html=renderThaiDocument(complete),sha256=createHash("sha256").update(html,"utf8").digest("hex"),fingerprint=taxSourceDigest(complete.source),now=Date.now();
  return {id,documentNumber:number,sha256,html,steps:[
    ...(snapshot.kind==="withholding_certificate"?[db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`finance-withholding-month:${actor.companyId}:${String(snapshot.paymentDate).slice(0,7)}:${snapshot.formKind}`)]:[]),
    ...(snapshot.kind==="withholding_certificate"?await currentTaxSignerGuards(db,actor,payer,commandId):[]),
    ...taxIdentityGuards(db,actor,commandId,[payer,payee]),
    db.prepare("INSERT INTO finance_thai_document_sequences(company_id,document_kind,fiscal_year,last_number) VALUES(?,?,?,0) ON CONFLICT DO NOTHING").bind(actor.companyId,snapshot.kind,year),
    assertion(db,commandId,"thai-document-number","EXISTS(SELECT 1 FROM finance_thai_document_sequences WHERE company_id=? AND document_kind=? AND fiscal_year=? AND last_number=?)",[actor.companyId,snapshot.kind,year,last]),
    db.prepare("UPDATE finance_thai_document_sequences SET last_number=last_number+1 WHERE company_id=? AND document_kind=? AND fiscal_year=? AND last_number=?").bind(actor.companyId,snapshot.kind,year,last),
    db.prepare("INSERT INTO finance_issued_thai_documents(id,company_id,document_kind,document_number,issue_date,payment_id,invoice_id,assessment_id,previous_document_id,edition,payer_identity_id,payee_identity_id,source_fingerprint,snapshot_json,export_html,export_sha256,reason,issued_by,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,snapshot.kind,number,snapshot.issueDate,input.paymentId??null,input.invoiceId??null,input.assessmentId??null,input.previous?.id??null,snapshot.edition,payer.id,payee.id,fingerprint,JSON.stringify(complete),html,sha256,snapshot.reason,actor.userId,commandId,now),
    db.prepare("INSERT INTO finance_thai_document_events(id,document_id,company_id,action,reason,actor_id,command_id,created_at) VALUES(?,?,?,'issued',?,?,?,?)").bind(`thai-event-${commandId}`,id,actor.companyId,snapshot.reason,actor.userId,commandId,now),
  ]};
}

export async function issueThaiTaxInvoice(actor:WorkContext,invoiceId:string,input:z.infer<typeof thaiInvoiceIssueSchema>,commandId:string){
  actor=requireCompanyTaxAuthority(actor,"finance.approve");input=thaiInvoiceIssueSchema.parse(input);
  const {write}=await holdingResources(actor.holdingId),operation="finance.thai-tax-invoice.issue",previous=await replayCommand(write,actor,commandId,operation,invoiceId,input);if(previous)return previous;
  const invoice=await write.prepare("SELECT i.*,j.status journal_status FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?").bind(invoiceId,actor.companyId).first<Record<string,string|number|null>>();
  if(!invoice)throw new ApiError(404,"TAX_INVOICE_SOURCE_NOT_FOUND","A posted Company invoice is required.");
  if(!["posted","partially_paid","paid"].includes(String(invoice.status))||invoice.journal_status!=="posted"||Number(invoice.version)!==input.expectedVersion)throw new ApiError(409,"TAX_INVOICE_SOURCE_CHANGED","Refresh the recognized invoice before issuing a VAT invoice.");
  if(invoice.kind!=="receivable"||invoice.currency!=="THB"||invoice.source_type!=="standalone"||invoice.line_mode!=="native_lines")throw new ApiError(422,"TAX_INVOICE_ADAPTER_UNSUPPORTED","The native ordinary VAT adapter requires an itemized standalone THB customer invoice; other source modules retain their owner-issued documents.");
  if(samePerson(actor,invoice.created_by))throw new ApiError(403,"INDEPENDENT_TAX_REVIEW_REQUIRED","A different Finance approver must issue the VAT invoice.");
  const date=String(invoice.invoice_date),payer=await reviewedTaxIdentity(write,actor,input.payerIdentityId,null,date),payee=await reviewedTaxIdentity(write,actor,input.payeeIdentityId,String(invoice.party_id),date);
  if(!payer.vat_registered)throw new ApiError(422,"VAT_REGISTRATION_REQUIRED","The issuer must have independently reviewed VAT registration evidence.");
  const rule=thaiFinanceTaxPack.ordinaryVat,lines=await currentInvoiceLines(write,actor.companyId,invoiceId,Number(invoice.line_revision));
  if(date<rule.effectiveFrom||date>rule.effectiveTo||!lines.length||lines.some(line=>line.origin_kind!=="native_line"||Number(line.tax_bps)!==rule.rateBps))throw new ApiError(422,"VAT_RULES_NOT_READY","This ordinary VAT adapter supports the source-dated 7% rate and explicit quantities; exemptions and special VAT regimes need their owner adapter.");
  if(await write.prepare("SELECT 1 FROM finance_issued_thai_documents WHERE invoice_id=? AND document_kind='tax_invoice'").bind(invoiceId).first())throw new ApiError(409,"TAX_INVOICE_ALREADY_ISSUED","Review a linked document amendment instead of issuing the same source again.");
  const issue=await prepareThaiDocumentIssue(write,actor,{payer,payee,invoiceId,commandId,snapshot:{schema:"finance.thai.document.v1",kind:"tax_invoice",issueDate:date,payer,payee,incomeMinor:Number(invoice.subtotal_minor),taxMinor:Number(invoice.tax_minor),grossMinor:Number(invoice.total_minor),items:lines.map(line=>({description:line.description,quantity:String(Number(line.quantity_millis)/1000),unitPriceMinor:Number(line.unit_price_minor),incomeMinor:Number(line.subtotal_minor),taxMinor:Number(line.tax_minor)})),source:{invoiceId,invoiceVersion:invoice.version,lineRevision:invoice.line_revision,journalId:invoice.journal_id,lineSources:lines.map(line=>({id:line.id,snapshot:JSON.parse(line.source_snapshot_json)})),rulePackVersion:thaiFinanceTaxPack.version,rulePackDigest:taxSourceDigest(thaiFinanceTaxPack)},edition:1,reason:input.reason}});
  return executeCommand(write,actor,commandId,operation,input,invoiceId,input.expectedVersion,{id:issue.id,documentNumber:issue.documentNumber,sha256:issue.sha256,status:"issued"},[
    assertion(write,commandId,"invoice-source","EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.status IN ('posted','partially_paid','paid') AND j.status='posted')",[invoiceId,actor.companyId,input.expectedVersion]),
    assertion(write,commandId,"single-document","NOT EXISTS(SELECT 1 FROM finance_issued_thai_documents WHERE invoice_id=? AND document_kind='tax_invoice')",[invoiceId]),...issue.steps,
  ]);
}

export async function thaiDocumentDetail(actor:WorkContext,id:string){
  actor=requireCompanyTaxAuthority(actor,"finance.read");const {write}=await holdingResources(actor.holdingId),document=await write.prepare("SELECT * FROM finance_issued_thai_documents WHERE id=? AND company_id=?").bind(id,actor.companyId).first<IssuedDocument>();
  if(!document)throw new ApiError(404,"THAI_DOCUMENT_NOT_FOUND","The issued document was not found in this Company.");
  const [events,changes,identities]=await Promise.all([
    write.prepare("SELECT * FROM finance_thai_document_events WHERE document_id=? AND company_id=? ORDER BY created_at,id").bind(id,actor.companyId).all(),
    write.prepare("SELECT * FROM finance_thai_document_changes WHERE document_id=? AND company_id=? ORDER BY created_at,id").bind(id,actor.companyId).all(),
    write.prepare("SELECT id,party_id,legal_name,tax_id,version FROM finance_tax_identities WHERE company_id=? AND status='approved' ORDER BY legal_name").bind(actor.companyId).all(),
  ]);
  const sourcePartyId=await write.prepare(document.payment_id?"SELECT party_id FROM finance_payments WHERE id=? AND company_id=?":"SELECT party_id FROM finance_invoices WHERE id=? AND company_id=?").bind(document.payment_id??document.invoice_id,actor.companyId).first<string>("party_id");
  return {document:{...document,export_html:undefined},snapshot:JSON.parse(document.snapshot_json) as ThaiDocumentSnapshot,sourcePartyId,events:events.results,changes:changes.results,identities:identities.results};
}

export async function thaiDocumentExport(actor:WorkContext,id:string){
  actor=requireCompanyTaxAuthority(actor,"finance.read");const {write}=await holdingResources(actor.holdingId),document=await write.prepare("SELECT export_html,export_sha256,document_number FROM finance_issued_thai_documents WHERE id=? AND company_id=?").bind(id,actor.companyId).first<{export_html:string;export_sha256:string;document_number:string}>();
  if(!document)throw new ApiError(404,"THAI_DOCUMENT_NOT_FOUND","The retained issued document was not found.");
  if(createHash("sha256").update(document.export_html,"utf8").digest("hex")!==document.export_sha256)throw new ApiError(503,"THAI_DOCUMENT_CHECKSUM_MISMATCH","The retained document does not match its immutable digest.");
  return {content:document.export_html,sha256:document.export_sha256,filename:`${document.document_number}.html`};
}

export async function prepareThaiDocumentChange(actor:WorkContext,documentId:string,input:z.infer<typeof thaiDocumentChangeSchema>,commandId:string){
  actor=requireCompanyTaxAuthority(actor,"finance.write");input=thaiDocumentChangeSchema.parse(input);
  const {write}=await holdingResources(actor.holdingId),id=`thai-document-change-${commandId}`,previous=await replayCommand(write,actor,commandId,"finance.thai-document.change.prepare",id,input);if(previous)return previous;
  const original=await write.prepare("SELECT * FROM finance_issued_thai_documents WHERE id=? AND company_id=?").bind(documentId,actor.companyId).first<IssuedDocument>();if(!original)throw new ApiError(404,"THAI_DOCUMENT_NOT_FOUND","Select an issued Company document.");
  if(input.issueDate<original.issue_date)throw new ApiError(422,"DOCUMENT_CHANGE_BEFORE_SOURCE","An amendment cannot precede the issued source.");
  const snapshot=JSON.parse(original.snapshot_json) as ThaiDocumentSnapshot,payer=await reviewedTaxIdentity(write,actor,input.payerIdentityId,null,input.issueDate),party=await write.prepare(original.payment_id?"SELECT party_id FROM finance_payments WHERE id=? AND company_id=?":"SELECT party_id FROM finance_invoices WHERE id=? AND company_id=?").bind(original.payment_id??original.invoice_id,actor.companyId).first<{party_id:string}>();
  if(!party)throw new ApiError(409,"THAI_DOCUMENT_SOURCE_CHANGED","The source legal party is unavailable.");
  const payee=await reviewedTaxIdentity(write,actor,input.payeeIdentityId,party.party_id,input.issueDate);
  if(original.document_kind==="tax_invoice"&&!payer.vat_registered)throw new ApiError(422,"VAT_REGISTRATION_REQUIRED","The corrected issuer requires reviewed VAT registration.");
  const changed={...snapshot,payer,payee,issueDate:input.issueDate,previousDocumentNumber:original.document_number,edition:Number(original.edition)+1,reason:input.reason},source={documentId,originalSha256:original.export_sha256,payerVersion:payer.version,payeeVersion:payee.version,snapshot:changed},now=Date.now();
  return executeCommand(write,actor,commandId,"finance.thai-document.change.prepare",input,id,1,{id,status:"draft",version:1},[
    ...taxIdentityGuards(write,actor,commandId,[payer,payee]),
    assertion(write,commandId,"current-document","NOT EXISTS(SELECT 1 FROM finance_thai_document_events WHERE document_id=? AND action IN ('cancelled','superseded'))",[documentId]),
    write.prepare("INSERT INTO finance_thai_document_changes(id,company_id,document_id,corrected_payer_identity_id,corrected_payee_identity_id,issue_date,reason,source_fingerprint,source_snapshot_json,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,documentId,payer.id,payee.id,input.issueDate,input.reason,taxSourceDigest(source),JSON.stringify(source),actor.userId,now,now),
  ]);
}

export async function decideThaiDocumentChange(actor:WorkContext,id:string,input:z.infer<typeof thaiDocumentDecisionSchema>,commandId:string){
  actor=requireCompanyTaxAuthority(actor,input.action==="cancel"?"finance.write":"finance.approve");input=thaiDocumentDecisionSchema.parse(input);
  const {write}=await holdingResources(actor.holdingId),operation=`finance.thai-document.change.${input.action}`,previous=await replayCommand(write,actor,commandId,operation,id,input);if(previous)return previous;
  const change=await write.prepare("SELECT * FROM finance_thai_document_changes WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Record<string,string|number|null>>();if(!change)throw new ApiError(404,"THAI_DOCUMENT_CHANGE_NOT_FOUND","The document amendment was not found.");
  if(change.status!=="draft"||Number(change.version)!==input.expectedVersion)throw new ApiError(409,"THAI_DOCUMENT_CHANGE_CHANGED","Refresh the pending document amendment.");
  if(input.action==="cancel"?!samePerson(actor,change.prepared_by):samePerson(actor,change.prepared_by))throw new ApiError(403,"INDEPENDENT_TAX_REVIEW_REQUIRED","Only the preparer may cancel; a different Finance approver must review this amendment.");
  const original=await write.prepare("SELECT * FROM finance_issued_thai_documents WHERE id=? AND company_id=?").bind(change.document_id,actor.companyId).first<IssuedDocument>();if(!original)throw new ApiError(409,"THAI_DOCUMENT_SOURCE_CHANGED","The immutable original document is unavailable.");
  const source=JSON.parse(String(change.source_snapshot_json)) as {originalSha256:string;payerVersion:number;payeeVersion:number;snapshot:ThaiDocumentSnapshot},now=Date.now();
  const guards=[assertion(write,commandId,"amendment","EXISTS(SELECT 1 FROM finance_thai_document_changes WHERE id=? AND company_id=? AND status='draft' AND version=?)",[id,actor.companyId,input.expectedVersion]),assertion(write,commandId,"current-document","NOT EXISTS(SELECT 1 FROM finance_thai_document_events WHERE document_id=? AND action IN ('cancelled','superseded'))",[original.id])];
  let issue:Awaited<ReturnType<typeof prepareThaiDocumentIssue>>|null=null;
  if(input.action==="approve"){
    const party=await write.prepare(original.payment_id?"SELECT party_id,status FROM finance_payments WHERE id=? AND company_id=?":"SELECT party_id,status FROM finance_invoices WHERE id=? AND company_id=?").bind(original.payment_id??original.invoice_id,actor.companyId).first<{party_id:string;status:string}>();
    if(!party||!(original.payment_id?["settled"]:["posted","partially_paid","paid"]).includes(party.status)||source.originalSha256!==original.export_sha256||taxSourceDigest(source)!==change.source_fingerprint)throw new ApiError(409,"THAI_DOCUMENT_SOURCE_CHANGED","The retained document or settled source changed before amendment review.");
    const payer=await reviewedTaxIdentity(write,actor,String(change.corrected_payer_identity_id),null,String(change.issue_date)),payee=await reviewedTaxIdentity(write,actor,String(change.corrected_payee_identity_id),party.party_id,String(change.issue_date));
    if(Number(payer.version)!==Number(source.payerVersion)||Number(payee.version)!==Number(source.payeeVersion))throw new ApiError(409,"THAI_DOCUMENT_IDENTITY_CHANGED","Review a new amendment against the current approved identities.");
    const snapshot={...source.snapshot,payer,payee,source:{...source.snapshot.source,previousDocumentId:original.id,previousSha256:original.export_sha256,amendmentId:id,payerVersion:payer.version,payeeVersion:payee.version}};
    issue=await prepareThaiDocumentIssue(write,actor,{payer,payee,snapshot,previous:original,paymentId:original.payment_id??undefined,invoiceId:original.invoice_id??undefined,assessmentId:original.assessment_id??undefined,commandId});
    guards.push(assertion(write,commandId,"financial-source",original.payment_id?"EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND status='settled')":"EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status IN ('posted','partially_paid','paid'))",[original.payment_id??original.invoice_id,actor.companyId]));
  }
  const status=input.action==="approve"?"issued":input.action==="reject"?"rejected":"cancelled";
  return executeCommand(write,actor,commandId,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1,replacementDocumentId:issue?.id??null},[
    ...guards,...(issue?.steps??[]),
    ...(issue?[write.prepare("INSERT INTO finance_thai_document_events(id,document_id,company_id,action,replacement_document_id,reason,actor_id,command_id,created_at) VALUES(?,?,?,'superseded',?,?,?,?,?)").bind(`thai-supersede-${commandId}`,original.id,actor.companyId,issue.id,input.reason,actor.userId,commandId,now)]:[]),
    write.prepare("UPDATE finance_thai_document_changes SET status=?,replacement_document_id=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?").bind(status,issue?.id??null,actor.userId,input.reason,now,now,id),
  ]);
}
