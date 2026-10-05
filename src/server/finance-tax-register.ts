import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import { samePerson } from "@/lib/iam";
import { minorToDecimal } from "@/lib/finance-money";
import type { WorkContext } from "./authorization";
import { requirePermission } from "./iam-permissions";
import { assertion,executeCommand,replayCommand } from "./commands";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import type { SqlDatabase,SqlStatement } from "./runtime/types";

export type TaxRow=Record<string,string|number|null>;
const month=z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/);
const reason=z.string().trim().min(10).max(4000);
const digest=(value:unknown)=>createHash("sha256").update(JSON.stringify(value)).digest("hex");
function companyFinance(actor:WorkContext,permission:"finance.write"|"finance.approve"){
  actor=requirePermission(actor,permission);
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_FINANCE_REQUIRED","Use a Company Finance assignment for the statutory register.");
  return actor;
}
function monthDates(value:string){
  month.parse(value);const [year,n]=value.split("-").map(Number);
  return {start:`${value}-01`,end:`${value}-${new Date(Date.UTC(year,n,0)).getUTCDate()}`};
}
async function profile(write:SqlDatabase,actor:WorkContext,chartId:string,date:string){
  const row=await write.prepare("SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date<=?").bind(actor.companyId,chartId,date).first<TaxRow>();
  if(!row)throw new ApiError(409,"STATUTORY_PROFILE_REQUIRED","Approve the Company's effective Thai statutory ledger and accounting policies first.");
  return row;
}

export const vatEntrySchema=z.object({
  sourceType:z.enum(["invoice","commerce_return","billing_credit"]),sourceId:z.string().min(1).max(128),expectedSourceVersion:z.number().int().positive(),
  taxPointDate:z.string().date(),claimMonth:month,documentReference:z.string().trim().min(1).max(200),
  treatment:z.enum(["standard","zero","exempt","disallowed","allocated"]),deductibleBps:z.number().int().min(0).max(10000),
  legalBasis:reason,taxPointEvidence:reason,documentEvidence:reason,
});
export const taxReviewSchema=z.object({expectedVersion:z.number().int().positive(),reason});
export const vatCorrectionSchema=vatEntrySchema.omit({sourceType:true,sourceId:true}).extend({expectedEntryVersion:z.number().int().positive(),reason});
export const vatPackageSchema=z.object({chartId:z.string().min(1),filingMonth:month,filingKind:z.enum(["original","additional"]),originalPackageId:z.string().min(1).optional(),reason});
export const vatFilingSchema=taxReviewSchema.extend({filingReference:z.string().trim().min(1).max(200),filedDate:z.string().date(),receiptEvidence:reason});

export const pendingVatReviewSql=`WITH scope AS (
  SELECT company_id,statutory_chart_id,effective_date,?::text end_date FROM finance_accounting_profiles WHERE company_id=? AND status='approved'
) SELECT COUNT(*) value FROM (
  SELECT i.id FROM scope s JOIN finance_invoices i ON i.company_id=s.company_id JOIN finance_journals j ON j.id=i.journal_id
  WHERE j.chart_id=s.statutory_chart_id AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')
  UNION ALL SELECT r.id FROM scope s JOIN commerce_returns r ON r.company_id=s.company_id JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
  WHERE r.status='posted' AND j.chart_id=s.statutory_chart_id AND j.status='posted' AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')
  UNION ALL SELECT r.id FROM scope s JOIN finance_billing_corrections r ON r.company_id=s.company_id JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
  WHERE r.status='posted' AND j.chart_id=s.statutory_chart_id AND j.status='posted' AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')
  UNION ALL SELECT c.id FROM scope s JOIN finance_vat_corrections c ON c.company_id=s.company_id AND c.chart_id=s.statutory_chart_id
    JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.status='prepared' AND t.accounting_date<=s.end_date
) pending`;

async function vatSource(write:SqlDatabase,actor:WorkContext,sourceType:"invoice"|"commerce_return"|"billing_credit",id:string){
  const source=await write.prepare(sourceType==="invoice"?
    `SELECT i.id,i.version,i.kind,i.invoice_number document_number,i.invoice_date document_date,i.currency,i.subtotal_minor,i.tax_minor,i.tax_account_id,i.journal_id,
      i.party_id,p.name party_name,p.tax_registration,p.version party_version,j.chart_id,j.entry_date accounting_date,j.status journal_status,j.currency journal_currency,c.base_currency,
      e.id financial_event_id,e.evidence_json financial_evidence
      FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=i.journal_id
      JOIN finance_charts c ON c.id=j.chart_id JOIN finance_financial_events e ON e.journal_id=j.id
      WHERE i.id=? AND i.company_id=?`:
    sourceType==='billing_credit'?`SELECT r.id,r.version,i.kind,r.number document_number,r.entry_date document_date,i.currency,r.subtotal_minor,r.tax_minor,i.tax_account_id,r.credit_journal_id journal_id,
      i.party_id,p.name party_name,p.tax_registration,p.version party_version,j.chart_id,j.entry_date accounting_date,j.status journal_status,j.currency journal_currency,c.base_currency,
      e.id financial_event_id,e.evidence_json financial_evidence,r.invoice_id
      FROM finance_billing_corrections r JOIN finance_invoices i ON i.id=r.invoice_id AND i.company_id=r.company_id JOIN finance_parties p ON p.id=i.party_id
      JOIN finance_journals j ON j.id=r.credit_journal_id JOIN finance_charts c ON c.id=j.chart_id JOIN finance_financial_events e ON e.journal_id=j.id
      WHERE r.id=? AND r.company_id=? AND r.status='posted'`:
    `SELECT r.id,r.version,i.kind,r.number document_number,r.return_date document_date,i.currency,r.subtotal_minor,r.tax_minor,i.tax_account_id,r.credit_journal_id journal_id,
      i.party_id,p.name party_name,p.tax_registration,p.version party_version,j.chart_id,j.entry_date accounting_date,j.status journal_status,j.currency journal_currency,c.base_currency,
      e.id financial_event_id,e.evidence_json financial_evidence,r.invoice_id
      FROM commerce_returns r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_parties p ON p.id=i.party_id
      JOIN finance_journals j ON j.id=r.credit_journal_id JOIN finance_charts c ON c.id=j.chart_id JOIN finance_financial_events e ON e.journal_id=j.id
      WHERE r.id=? AND r.company_id=? AND r.status='posted'`).bind(id,actor.companyId).first<TaxRow>();
  if(!source||!['posted','reversed'].includes(String(source.journal_status)))throw new ApiError(404,"POSTED_TAX_SOURCE_REQUIRED","A posted document and its immutable journal evidence are required.");
  if(source.currency!=="THB"||source.journal_currency!=="THB"||source.base_currency!=="THB")throw new ApiError(422,"VAT_FX_ADAPTER_REQUIRED","This register currently accepts THB documents in the approved THB statutory ledger.");
  if(!source.tax_account_id)throw new ApiError(422,"TAX_ACCOUNT_REQUIRED","The originating document must identify its posted VAT account.");
  const negative=sourceType!=="invoice",sign=negative?-1:1,direction=source.kind==="receivable"?"output":"input";
  const evidence=JSON.parse(String(source.financial_evidence)) as {lines:Array<{account_id:string;base_debit_minor:number;base_credit_minor:number}>};
  const ledger=evidence.lines.filter(l=>l.account_id===source.tax_account_id).reduce((n,l)=>n+(direction==="output"?Number(l.base_credit_minor)-Number(l.base_debit_minor):Number(l.base_debit_minor)-Number(l.base_credit_minor)),0);
  if(ledger!==sign*Number(source.tax_minor))throw new ApiError(409,"TAX_SOURCE_GL_MISMATCH","Document VAT does not agree with its posted tax-account lines. Correct the originating evidence first.");
  return {source,sourceHash:digest(source),direction,sign,ledger};
}
function sourceGuards(write:SqlDatabase,actor:WorkContext,key:string,type:"invoice"|"commerce_return"|"billing_credit",source:TaxRow,prefix=""){
  const table=type==="invoice"?"finance_invoices":type==="billing_credit"?"finance_billing_corrections":"commerce_returns";
  return [
    assertion(write,key,`${prefix}tax-source-version`,`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,[source.id,actor.companyId,source.version]),
    assertion(write,key,`${prefix}tax-party-version`,"EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND version=?)",[source.party_id,actor.companyId,source.party_version]),
    assertion(write,key,`${prefix}statutory-profile`,"EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date<=?)",[actor.companyId,source.chart_id,source.accounting_date]),
  ];
}
function decision(write:SqlDatabase,actor:WorkContext,key:string,id:string,operation:string,version:number,input:unknown){
  return write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,id,operation,actor.userId,actor.assignmentId,version,digest(input),Date.now());
}

function validateVatFacts(input:z.infer<typeof vatEntrySchema>,direction:string,ledger:number,source:TaxRow,original:TaxRow|null){
  if(input.claimMonth<input.taxPointDate.slice(0,7)||(direction==="output"&&input.claimMonth!==input.taxPointDate.slice(0,7)))throw new ApiError(422,"VAT_CLAIM_MONTH_INVALID","Output VAT belongs to its tax-point month; input VAT cannot be claimed before that month.");
  if(direction==="output"&&(["disallowed","allocated"].includes(input.treatment)||input.deductibleBps!==10000))throw new ApiError(422,"OUTPUT_VAT_TREATMENT_INVALID","Input-tax deductions do not apply to output VAT.");
  if(["zero","exempt"].includes(input.treatment)&&ledger!==0)throw new ApiError(422,"VAT_TREATMENT_MISMATCH","A zero-rated or exempt document must have zero posted VAT.");
  const expectedDeduction=input.treatment==="disallowed"?0:10000;
  if(input.treatment!=="allocated"&&input.deductibleBps!==expectedDeduction)throw new ApiError(422,"VAT_DEDUCTION_INVALID","Use the treatment's full or prohibited deduction; proportional deduction requires allocated treatment.");
  if(input.sourceType!=="invoice"){
    if(!original)throw new ApiError(409,"ORIGINAL_TAX_ENTRY_REQUIRED","Review the original invoice's tax entry before its credit/debit note.");
    if(input.taxPointDate<String(original.tax_point_date)||input.taxPointDate<String(source.document_date))throw new ApiError(422,"RETURN_TAX_DATE_INVALID","A return tax adjustment cannot precede its original VAT point or return document date.");
    if(original.treatment!==input.treatment||original.deductible_bps!==input.deductibleBps)throw new ApiError(422,"RETURN_VAT_POLICY_MISMATCH","The return must retain the original document's reviewed deduction policy.");
  }
}
function insertTaxEntry(write:SqlDatabase,actor:WorkContext,id:string,input:z.infer<typeof vatEntrySchema>,current:Awaited<ReturnType<typeof vatSource>>,original:TaxRow|null,correction?:{id:string;replacesId:string;reason:string}){
  const {source,sourceHash,sign,ledger,direction}=current;
  const deductible=sign*Math.round(Math.abs(ledger)*input.deductibleBps/10000),evidence={facts:input,source,originalDocumentReference:original?.document_reference??null};
  return write.prepare(`INSERT INTO finance_tax_entries(id,company_id,chart_id,financial_event_id,tax_account_id,source_type,source_id,source_version,original_entry_id,direction,document_date,accounting_date,tax_point_date,claim_month,document_reference,treatment,deductible_bps,taxable_minor,tax_minor,deductible_minor,source_hash,evidence_json,prepared_by,created_at,replaces_entry_id,correction_id)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(id,actor.companyId,source.chart_id,source.financial_event_id,source.tax_account_id,input.sourceType,input.sourceId,source.version,original?.id??null,direction,source.document_date,source.accounting_date,input.taxPointDate,input.claimMonth,input.documentReference,input.treatment,input.deductibleBps,sign*Number(source.subtotal_minor),ledger,deductible,sourceHash,JSON.stringify({...evidence,correction:correction??null}),actor.userId,Date.now(),correction?.replacesId??null,correction?.id??null);
}
async function originalVatEntry(write:SqlDatabase,actor:WorkContext,source:TaxRow){
  return write.prepare("SELECT * FROM finance_current_tax_entries WHERE company_id=? AND source_type='invoice' AND source_id=? AND status='approved'").bind(actor.companyId,source.invoice_id).first<TaxRow>();
}
export async function prepareVatEntry(actor:WorkContext,raw:z.infer<typeof vatEntrySchema>,key:string){
  actor=companyFinance(actor,"finance.write");const input=vatEntrySchema.parse(raw),{write}=await holdingResources(actor.holdingId),id=`tax-entry-${key}`;
  const replay=await replayCommand(write,actor,key,"finance.tax.prepare",id,input);if(replay)return replay;
  const current=await vatSource(write,actor,input.sourceType,input.sourceId),{source,direction,sign,ledger}=current;
  if(Number(source.version)!==input.expectedSourceVersion)throw new ApiError(409,"VERSION_CONFLICT","The originating document changed. Review its latest evidence.");
  await profile(write,actor,String(source.chart_id),String(source.accounting_date));
  const original=input.sourceType!=="invoice"?await originalVatEntry(write,actor,source):null;
  validateVatFacts(input,direction,ledger,source,original);
  const deductible=sign*Math.round(Math.abs(ledger)*input.deductibleBps/10000);
  const familyId=input.sourceType==="invoice"?source.id:source.invoice_id;
  return executeCommand(write,actor,key,"finance.tax.prepare",input,id,1,{id,status:"prepared",version:1,taxMinor:ledger,deductibleMinor:deductible},[
    ...sourceGuards(write,actor,key,input.sourceType,source),
    assertion(write,key,"tax-family-correction","NOT EXISTS(SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared')",[actor.companyId,familyId]),
    insertTaxEntry(write,actor,id,input,current,original),
  ]);
}

export async function reviewVatEntry(actor:WorkContext,id:string,action:"approve"|"reject",raw:z.infer<typeof taxReviewSchema>,key:string){
  actor=companyFinance(requirePermission(actor,"finance.write"),"finance.approve");const input=taxReviewSchema.parse(raw),{write}=await holdingResources(actor.holdingId),operation=`finance.tax.${action}`;
  const replay=await replayCommand(write,actor,key,operation,id,input);if(replay)return replay;
  const entry=await write.prepare("SELECT * FROM finance_tax_entries WHERE id=? AND company_id=?").bind(id,actor.companyId).first<TaxRow>();
  if(!entry)throw new ApiError(404,"TAX_ENTRY_NOT_FOUND","Tax entry not found in this Company.");
  if(entry.correction_id)throw new ApiError(409,"VAT_CORRECTION_WORKFLOW_REQUIRED","Review the complete linked correction together; individual replacement review is unavailable.");
  if(entry.status!=="prepared"||entry.version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Refresh the tax entry before reviewing it.");
  if(samePerson(actor,entry.prepared_by))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different Finance person must review tax facts and source evidence.");
  const steps:SqlStatement[]=[];
  if(action==="approve"){
    const current=await vatSource(write,actor,entry.source_type as "invoice"|"commerce_return"|"billing_credit",String(entry.source_id));
    if(current.sourceHash!==entry.source_hash)throw new ApiError(409,"TAX_SOURCE_CHANGED","The source changed after preparation. Reject this draft and prepare the current source.");
    steps.push(...sourceGuards(write,actor,key,entry.source_type as "invoice"|"commerce_return"|"billing_credit",current.source));
  }
  const status=action==="approve"?"approved":"rejected";
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},[
    assertion(write,key,"tax-entry","EXISTS(SELECT 1 FROM finance_tax_entries WHERE id=? AND company_id=? AND version=? AND status='prepared')",[id,actor.companyId,input.expectedVersion]),...steps,
    write.prepare("UPDATE finance_tax_entries SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?").bind(status,actor.userId,input.reason,Date.now(),id),
    decision(write,actor,key,id,operation,input.expectedVersion,input),
  ]);
}

const familyEvidenceSql=`jsonb_build_object(
  'returns',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'version',r.version,'status',r.status,'journal',r.credit_journal_id) ORDER BY r.id) FROM commerce_returns r WHERE r.company_id=? AND r.invoice_id=?),'[]'::jsonb),
  'billingCredits',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'version',r.version,'status',r.status,'journal',r.credit_journal_id) ORDER BY r.id) FROM finance_billing_corrections r WHERE r.company_id=? AND r.invoice_id=?),'[]'::jsonb),
  'entries',COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.company_id=? AND
    ((t.source_type='invoice' AND t.source_id=?) OR (t.source_type='commerce_return' AND t.source_id IN (SELECT id FROM commerce_returns WHERE company_id=? AND invoice_id=?)) OR (t.source_type='billing_credit' AND t.source_id IN (SELECT id FROM finance_billing_corrections WHERE company_id=? AND invoice_id=?)))),'[]'::jsonb))::text`;
async function familyEvidence(write:SqlDatabase,actor:WorkContext,familyId:string){
  return String(await write.prepare(`SELECT ${familyEvidenceSql} value`).bind(actor.companyId,familyId,actor.companyId,familyId,actor.companyId,familyId,actor.companyId,familyId,actor.companyId,familyId).first("value"));
}
function familyGuard(write:SqlDatabase,actor:WorkContext,key:string,familyId:string,json:string){
  return assertion(write,key,"vat-correction-family",`(${familyEvidenceSql})::jsonb=?::jsonb`,[actor.companyId,familyId,actor.companyId,familyId,actor.companyId,familyId,actor.companyId,familyId,actor.companyId,familyId,json]);
}
async function correctionRow(write:SqlDatabase,actor:WorkContext,id:string){
  const row=await write.prepare("SELECT * FROM finance_vat_corrections WHERE id=? AND company_id=?").bind(id,actor.companyId).first<TaxRow>();
  if(!row)throw new ApiError(404,"VAT_CORRECTION_NOT_FOUND","VAT correction not found in this Company.");return row;
}
export async function prepareVatCorrection(actor:WorkContext,entryId:string,raw:z.infer<typeof vatCorrectionSchema>,key:string){
  actor=companyFinance(actor,"finance.write");const input=vatCorrectionSchema.parse(raw),{write}=await holdingResources(actor.holdingId),id=`vat-correction-${key}`,operation="finance.vat-correction.prepare";
  const replay=await replayCommand(write,actor,key,operation,id,{entryId,...input});if(replay)return replay;
  const previous=await write.prepare("SELECT * FROM finance_current_tax_entries WHERE id=? AND company_id=? AND status='approved'").bind(entryId,actor.companyId).first<TaxRow>();
  if(!previous)throw new ApiError(404,"CURRENT_APPROVED_TAX_ENTRY_REQUIRED","Select the latest approved entry in this Company.");
  if(Number(previous.version)!==input.expectedEntryVersion)throw new ApiError(409,"VERSION_CONFLICT","The reviewed entry changed. Refresh its current revision.");
  const current=await vatSource(write,actor,previous.source_type as "invoice"|"commerce_return"|"billing_credit",String(previous.source_id));
  if(Number(current.source.version)!==input.expectedSourceVersion)throw new ApiError(409,"VERSION_CONFLICT","The originating document changed. Review its latest evidence.");
  await profile(write,actor,String(previous.chart_id),String(previous.accounting_date));
  const familyId=String(previous.source_type==="invoice"?previous.source_id:current.source.invoice_id),json=await familyEvidence(write,actor,familyId),family=JSON.parse(json) as {entries:TaxRow[];returns:Array<{id:string;status:string}>;billingCredits:Array<{id:string;status:string}>};
  if(await write.prepare("SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared'").bind(actor.companyId,familyId).first())throw new ApiError(409,"VAT_CORRECTION_PENDING","Review or reject the pending correction for this document family first.");
  if(family.billingCredits.some(r=>r.status==='posted'&&!family.entries.some(e=>e.source_type==='billing_credit'&&e.source_id===r.id))||family.entries.some(e=>e.status==="prepared")||family.returns.some(r=>r.status==="posted"&&!family.entries.some(e=>e.source_type==="commerce_return"&&e.source_id===r.id)))throw new ApiError(409,"VAT_FAMILY_REVIEW_REQUIRED","Review every posted invoice/return VAT entry before correcting this document family.");
  if(family.returns.length>1000)throw new ApiError(422,"VAT_CORRECTION_LIMIT","This document family requires a larger-volume correction adapter.");
  const related=previous.source_type==="invoice"?family.entries.filter(e=>e.source_type!=="invoice"):[];
  if(related.length>=100)throw new ApiError(422,"VAT_CORRECTION_LIMIT","A correction may replace at most 100 linked VAT entries together.");
  const facts=vatEntrySchema.parse({...input,sourceType:previous.source_type,sourceId:previous.source_id}),original=previous.source_type!=="invoice"?await originalVatEntry(write,actor,current.source):null;
  validateVatFacts(facts,current.direction,current.ledger,current.source,original);
  const members=[{previous,current,facts,original,id:`${id}-entry-0`}];
  const replacementOriginal:TaxRow={...previous,id:members[0].id,tax_point_date:facts.taxPointDate,claim_month:facts.claimMonth,document_reference:facts.documentReference,treatment:facts.treatment,deductible_bps:facts.deductibleBps};
  for(const old of related){
    const source=await vatSource(write,actor,old.source_type as "commerce_return"|"billing_credit",String(old.source_id)),oldFacts=JSON.parse(String(old.evidence_json)).facts;
    const newFacts=vatEntrySchema.parse({...oldFacts,sourceType:old.source_type,sourceId:old.source_id,expectedSourceVersion:Number(source.source.version),treatment:facts.treatment,deductibleBps:facts.deductibleBps,legalBasis:facts.legalBasis});
    validateVatFacts(newFacts,source.direction,source.ledger,source.source,replacementOriginal);
    members.push({previous:old,current:source,facts:newFacts,original:replacementOriginal,id:`${id}-entry-${members.length}`});
  }
  return executeCommand(write,actor,key,operation,{entryId,...input},id,1,{id,status:"prepared",version:1,entryCount:members.length},[
    familyGuard(write,actor,key,familyId,json),
    assertion(write,key,"vat-correction-current","EXISTS(SELECT 1 FROM finance_current_tax_entries WHERE id=? AND company_id=? AND version=? AND status='approved')",[entryId,actor.companyId,input.expectedEntryVersion]),
    assertion(write,key,"vat-correction-volume","(SELECT COUNT(*) FROM finance_vat_corrections WHERE company_id=? AND status='prepared')<100",[actor.companyId]),
    write.prepare("INSERT INTO finance_vat_corrections(id,company_id,chart_id,root_entry_id,family_source_id,reason,family_evidence_json,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,previous.chart_id,entryId,familyId,input.reason,json,actor.userId,Date.now()),
    ...members.flatMap((member,index)=>[
      ...sourceGuards(write,actor,key,member.facts.sourceType,member.current.source,`replacement-${index}-`),
      insertTaxEntry(write,actor,member.id,member.facts,member.current,member.original,{id,replacesId:String(member.previous.id),reason:input.reason}),
    ]),
  ]);
}
export async function reviewVatCorrection(actor:WorkContext,id:string,action:"approve"|"reject",raw:z.infer<typeof taxReviewSchema>,key:string){
  actor=companyFinance(requirePermission(actor,"finance.write"),"finance.approve");const input=taxReviewSchema.parse(raw),{write}=await holdingResources(actor.holdingId),operation=`finance.vat-correction.${action}`;
  const replay=await replayCommand(write,actor,key,operation,id,input);if(replay)return replay;
  const correction=await correctionRow(write,actor,id);
  if(correction.status!=="prepared"||Number(correction.version)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Refresh the correction before reviewing it.");
  if(samePerson(actor,correction.prepared_by))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different Finance person must review the complete linked correction.");
  const entries=(await write.prepare("SELECT * FROM finance_tax_entries WHERE correction_id=? ORDER BY id").bind(id).all<TaxRow>()).results;
  if(!entries.length||entries.length>100||entries.some(e=>e.status!=="prepared"))throw new ApiError(409,"VAT_CORRECTION_INCOMPLETE","The correction's replacement entries are incomplete.");
  const steps:SqlStatement[]=[];
  if(action==="approve"){
    const json=await familyEvidence(write,actor,String(correction.family_source_id));
    if(json!==correction.family_evidence_json)throw new ApiError(409,"VAT_CORRECTION_STALE","The source family changed. Reject this correction and prepare the current evidence.");
    steps.push(familyGuard(write,actor,key,String(correction.family_source_id),json));
    for(const [index,entry] of entries.entries()){
      const current=await vatSource(write,actor,entry.source_type as "invoice"|"commerce_return"|"billing_credit",String(entry.source_id));
      if(current.sourceHash!==entry.source_hash)throw new ApiError(409,"TAX_SOURCE_CHANGED","A source changed after preparation. Reject the complete correction and prepare it again.");
      steps.push(...sourceGuards(write,actor,key,entry.source_type as "invoice"|"commerce_return"|"billing_credit",current.source,`replacement-${index}-`));
    }
  }
  const status=action==="approve"?"approved":"rejected",now=Date.now();
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1,entryCount:entries.length},[
    assertion(write,key,"vat-correction-version","EXISTS(SELECT 1 FROM finance_vat_corrections WHERE id=? AND company_id=? AND status='prepared' AND version=?)",[id,actor.companyId,input.expectedVersion]),
    assertion(write,key,"vat-correction-entries","COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_tax_entries t WHERE correction_id=?),'[]'::jsonb)=?::jsonb",[id,JSON.stringify(entries)]),...steps,
    write.prepare("UPDATE finance_tax_entries SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE correction_id=? AND status='prepared'").bind(status,actor.userId,input.reason,now,id),
    write.prepare("UPDATE finance_vat_corrections SET status=?,version=version+1,reviewed_by=?,review_reason=?,reviewed_at=? WHERE id=?").bind(status,actor.userId,input.reason,now,id),
    decision(write,actor,key,id,operation,input.expectedVersion,input),
  ]);
}

// The hash covers the complete reviewed register for the month. List limits in
// the workspace do not truncate calculation or source-change checks.
const registerJson=`COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.company_id=? AND t.chart_id=? AND t.claim_month=?),'[]'::jsonb)::text`;
async function register(write:SqlDatabase,actor:WorkContext,chartId:string,filingMonth:string){
  const json=String(await write.prepare(`SELECT ${registerJson} value`).bind(actor.companyId,chartId,filingMonth).first("value"));
  const entries=JSON.parse(json) as TaxRow[];
  if(entries.length>1000)throw new ApiError(422,"VAT_PACKAGE_LIMIT","Split/review this register through a larger-volume adapter before preparing more than 1,000 records.");
  return {json,entries,sourceHash:digest(JSON.parse(json))};
}
const reconciliationSql=`WITH tax_accounts AS (
      SELECT DISTINCT i.tax_account_id id,CASE WHEN i.kind='receivable' THEN 'output' ELSE 'input' END direction
      FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.company_id=? AND j.chart_id=? AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    ) SELECT a.id,a.direction,ac.code,ac.name,
      COALESCE((SELECT SUM(CASE WHEN a.direction='output' THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END)
        FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=a.id AND j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN ? AND ?),0) ledger_minor,
      COALESCE((SELECT SUM(t.tax_minor) FROM finance_current_tax_entries t WHERE t.tax_account_id=a.id AND t.company_id=? AND t.chart_id=? AND t.direction=a.direction AND t.status='approved' AND t.accounting_date BETWEEN ? AND ?),0) register_minor,
      COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.tax_account_id=a.id AND t.company_id=? AND t.chart_id=? AND t.direction=a.direction AND t.status='approved' AND t.accounting_date BETWEEN ? AND ?),'[]'::jsonb)::text register_evidence_json
      FROM tax_accounts a JOIN finance_accounts ac ON ac.id=a.id ORDER BY a.id,a.direction`;
const missingSql=`SELECT COUNT(*) FROM (SELECT i.id FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id
    WHERE i.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')
    UNION ALL SELECT r.id FROM commerce_returns r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
    WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')
    UNION ALL SELECT r.id FROM finance_billing_corrections r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
    WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND j.entry_date BETWEEN ? AND ? AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')) pending`;
const pendingCorrectionsSql="SELECT COUNT(*) FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? AND c.status='prepared' AND t.accounting_date<=?";
async function vatReconciliation(write:SqlDatabase,actor:WorkContext,chartId:string,endDate:string,effectiveDate:string){
  const args=[actor.companyId,chartId,effectiveDate,endDate];
  const rows=(await write.prepare(reconciliationSql).bind(...args,...args,...args,...args).all<TaxRow>()).results;
  const missing=Number(await write.prepare(missingSql).bind(...args,...args,...args).first("count")??0);
  const pendingCorrections=Number(await write.prepare(pendingCorrectionsSql).bind(actor.companyId,chartId,endDate).first("count")??0);
  return {rows,missing,pendingCorrections,ready:missing===0&&pendingCorrections===0&&rows.every(r=>Number(r.ledger_minor)===Number(r.register_minor))};
}
function reconciliationGuards(write:SqlDatabase,actor:WorkContext,key:string,chartId:string,endDate:string,effectiveDate:string,rows:TaxRow[]){
  const args=[actor.companyId,chartId,effectiveDate,endDate];
  return [
    assertion(write,key,"vat-gl-evidence",`COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id,r.direction) FROM (${reconciliationSql}) r),'[]'::jsonb)=?::jsonb`,[...args,...args,...args,...args,JSON.stringify(rows)]),
    assertion(write,key,"vat-missing-sources",`(${missingSql})=0`,[...args,...args,...args]),
    assertion(write,key,"vat-pending-corrections",`(${pendingCorrectionsSql})=0`,[actor.companyId,chartId,endDate]),
  ];
}
export type VatCloseEvidence={profileId:string;chartId:string;effectiveDate:string;endDate:string;rows:TaxRow[];missing:number;pendingCorrections:number;ready:boolean};
export async function calculateVatCloseEvidence(write:SqlDatabase,actor:WorkContext,endDate:string):Promise<VatCloseEvidence|null>{
  const p=await write.prepare("SELECT id,statutory_chart_id,effective_date FROM finance_accounting_profiles WHERE company_id=? AND status='approved' AND effective_date<=?").bind(actor.companyId,endDate).first<TaxRow>();
  if(!p)return null;
  const reconciliation=await vatReconciliation(write,actor,String(p.statutory_chart_id),endDate,String(p.effective_date));
  return {profileId:String(p.id),chartId:String(p.statutory_chart_id),effectiveDate:String(p.effective_date),endDate,...reconciliation};
}
export function vatCloseGuards(write:SqlDatabase,actor:WorkContext,key:string,endDate:string,evidence:VatCloseEvidence|null){
  if(!evidence)return [assertion(write,key,"close-vat-profile-absent","NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved' AND effective_date<=?)",[actor.companyId,endDate])];
  return [assertion(write,key,"close-vat-profile","EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND statutory_chart_id=? AND status='approved' AND effective_date=?)",[evidence.profileId,actor.companyId,evidence.chartId,evidence.effectiveDate]),
    ...reconciliationGuards(write,actor,key,evidence.chartId,evidence.endDate,evidence.effectiveDate,evidence.rows)];
}
function registerGuard(write:SqlDatabase,actor:WorkContext,key:string,p:TaxRow,json:string){
  return assertion(write,key,"vat-register",`(${registerJson})::jsonb=?::jsonb`,[actor.companyId,p.chart_id,p.filing_month,json]);
}
function packageEvent(write:SqlDatabase,actor:WorkContext,key:string,id:string,version:number,action:string,evidence:unknown){
  return write.prepare("INSERT INTO finance_statutory_package_events VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,key,version,action,actor.userId,JSON.stringify(evidence),Date.now());
}
async function packageRow(write:SqlDatabase,actor:WorkContext,id:string){
  const row=await write.prepare("SELECT * FROM finance_statutory_packages WHERE id=? AND company_id=? AND form_code='PP30' AND filing_month IS NOT NULL").bind(id,actor.companyId).first<TaxRow>();
  if(!row)throw new ApiError(404,"VAT_PACKAGE_NOT_FOUND","VAT preparation package not found in this Company.");return row;
}

export async function prepareVatPackage(actor:WorkContext,raw:z.infer<typeof vatPackageSchema>,key:string){
  actor=companyFinance(actor,"finance.write");const input=vatPackageSchema.parse(raw),{write}=await holdingResources(actor.holdingId),id=`vat-package-${key}`,operation="finance.vat-package.prepare";
  const replay=await replayCommand(write,actor,key,operation,id,input);if(replay)return replay;
  const dates=monthDates(input.filingMonth),p=await profile(write,actor,input.chartId,dates.start),r=await register(write,actor,input.chartId,input.filingMonth);
  if(r.entries.some(e=>e.status!=="approved"))throw new ApiError(409,"VAT_REVIEW_REQUIRED","Review or reject every prepared entry in the filing month first.");
  const reconciliation=await vatReconciliation(write,actor,input.chartId,dates.end,String(p.effective_date));
  if(!reconciliation.ready)throw new ApiError(409,"VAT_GL_RECONCILIATION_REQUIRED","Review all source VAT records and resolve tax-account differences before preparing the return.");
  let original:TaxRow|null=null;
  if(input.filingKind==="additional"){
    if(!input.originalPackageId)throw new ApiError(422,"ORIGINAL_PACKAGE_REQUIRED","Select the previously filed original return.");
    original=await packageRow(write,actor,input.originalPackageId);
    if(!["filed","paid"].includes(String(original.status))||original.chart_id!==input.chartId||original.filing_month!==input.filingMonth||original.filing_kind!=="original")throw new ApiError(409,"ORIGINAL_PACKAGE_NOT_FILED","An additional return must reference the filed original for the same ledger and month.");
  }else if(input.originalPackageId)throw new ApiError(422,"ORIGINAL_PACKAGE_INVALID","An original return cannot reference another package.");
  const total=(direction:string,column:string)=>r.entries.filter(e=>e.direction===direction).reduce((n,e)=>n+Number(e[column]),0),outputVat=total("output","tax_minor"),inputVat=total("input","tax_minor"),deductibleInput=total("input","deductible_minor"),net=outputVat-deductibleInput;
  const result={outputVatMinor:outputVat,inputVatMinor:inputVat,deductibleInputMinor:deductibleInput,disallowedInputMinor:inputVat-deductibleInput,netMinor:net,payableMinor:Math.max(0,net),creditMinor:Math.max(0,-net)};
  const snapshot={register:r.entries,reconciliation,profileId:p.id,result,priorFiledResult:original?JSON.parse(String(original.snapshot_json)).result:null,preparationOnly:true,officialFormatValidated:false,priorCreditsIncluded:false};
  const runId=`vat-calc-${key}`,now=Date.now();
  return executeCommand(write,actor,key,operation,input,id,1,{id,status:"prepared",version:1,result},[
    registerGuard(write,actor,key,{chart_id:input.chartId,filing_month:input.filingMonth},r.json),
    ...reconciliationGuards(write,actor,key,input.chartId,dates.end,String(p.effective_date),reconciliation.rows),
    assertion(write,key,"vat-profile","EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND status='approved')",[p.id,actor.companyId]),
    write.prepare("INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_version,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,prepared_by,created_at,updated_at) VALUES(?,?,?,'thai_statutory',1,'THB',2,?,?,?,?,'prepared',?,?,?)")
      .bind(runId,actor.companyId,actor.holdingId,r.sourceHash,JSON.stringify({registerEntryIds:r.entries.map(e=>e.id)}),JSON.stringify(result),JSON.stringify({engine:"source-vat-register-v1",preparationOnly:true,priorCreditsIncluded:false}),actor.userId,now,now),
    write.prepare("INSERT INTO finance_statutory_packages(id,company_id,jurisdiction,form_code,filing_kind,calculation_run_id,status,schema_version,effective_date,prepared_by,created_at,updated_at,chart_id,filing_month,source_hash,snapshot_json,original_package_id,evidence_json) VALUES(?,?,'TH','PP30',?,?,'prepared','erpx-pp30-workpaper-v1',?,?,?,?,?,?,?,?,?,?)")
      .bind(id,actor.companyId,input.filingKind,runId,dates.end,actor.userId,now,now,input.chartId,input.filingMonth,r.sourceHash,JSON.stringify(snapshot),original?.id??null,JSON.stringify({preparationReason:input.reason})),
    packageEvent(write,actor,key,id,1,"prepare",{sourceHash:r.sourceHash,reason:input.reason}),
  ]);
}

export async function approveVatPackage(actor:WorkContext,id:string,raw:z.infer<typeof taxReviewSchema>,key:string){
  actor=companyFinance(requirePermission(actor,"finance.write"),"finance.approve");const input=taxReviewSchema.parse(raw),{write}=await holdingResources(actor.holdingId),operation="finance.vat-package.approve";
  const replay=await replayCommand(write,actor,key,operation,id,input);if(replay)return replay;
  const p=await packageRow(write,actor,id);
  if(p.status!=="prepared"||p.version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Refresh the package before review.");
  if(samePerson(actor,p.prepared_by))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different Finance approver must review the VAT package.");
  const r=await register(write,actor,String(p.chart_id),String(p.filing_month));
  if(r.sourceHash!==p.source_hash)throw new ApiError(409,"VAT_PACKAGE_STALE","The register changed after preparation. Supersede this package and prepare a new version.");
  const snapshot=JSON.parse(String(p.snapshot_json)),currentProfile=await profile(write,actor,String(p.chart_id),monthDates(String(p.filing_month)).start);
  const current=await vatReconciliation(write,actor,String(p.chart_id),String(p.effective_date),String(currentProfile.effective_date));
  if(!current.ready||digest(current)!==digest(snapshot.reconciliation))throw new ApiError(409,"VAT_PACKAGE_STALE","The posted VAT evidence changed after preparation.");
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:"approved_for_export",version:input.expectedVersion+1},[
    assertion(write,key,"vat-package","EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND status='prepared' AND version=?)",[id,actor.companyId,input.expectedVersion]),
    assertion(write,key,"vat-calculation","EXISTS(SELECT 1 FROM finance_calculation_runs WHERE id=? AND company_id=? AND status='prepared' AND version=1)",[p.calculation_run_id,actor.companyId]),
    registerGuard(write,actor,key,p,r.json),
    ...reconciliationGuards(write,actor,key,String(p.chart_id),String(p.effective_date),String(currentProfile.effective_date),current.rows),
    write.prepare("UPDATE finance_statutory_packages SET status='approved_for_export',version=version+1,reviewed_by=?,approved_by=?,review_reason=?,updated_at=? WHERE id=?").bind(actor.userId,actor.userId,input.reason,Date.now(),id),
    write.prepare("UPDATE finance_calculation_runs SET status='approved',version=version+1,reviewed_by=?,approved_by=?,updated_at=? WHERE id=? AND status='prepared'").bind(actor.userId,actor.userId,Date.now(),p.calculation_run_id),
    decision(write,actor,key,id,operation,input.expectedVersion,input),packageEvent(write,actor,key,id,input.expectedVersion+1,"approve",input),
  ]);
}

export async function exportVatWorkpaper(actor:WorkContext,id:string,raw:z.infer<typeof taxReviewSchema>,key:string){
  actor=companyFinance(actor,"finance.write");const input=taxReviewSchema.parse(raw),{write}=await holdingResources(actor.holdingId),operation="finance.vat-package.export";
  const replay=await replayCommand(write,actor,key,operation,id,input);if(replay)return replay;
  const p=await packageRow(write,actor,id);
  if(p.status!=="approved_for_export"||p.version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Only the current independently approved package can be exported.");
  const r=await register(write,actor,String(p.chart_id),String(p.filing_month));if(r.sourceHash!==p.source_hash)throw new ApiError(409,"VAT_PACKAGE_STALE","The register changed; prepare and review a replacement package.");
  // This is an ERPX workpaper, deliberately not labelled as an RD import file.
  const snapshot=JSON.parse(String(p.snapshot_json)),currentProfile=await profile(write,actor,String(p.chart_id),monthDates(String(p.filing_month)).start),current=await vatReconciliation(write,actor,String(p.chart_id),String(p.effective_date),String(currentProfile.effective_date));
  if(!current.ready||digest(current)!==digest(snapshot.reconciliation))throw new ApiError(409,"VAT_PACKAGE_STALE","The posted VAT evidence changed after approval.");
  // Prefix potentially executable spreadsheet cells; document references and
  // reviewer notes are untrusted text even inside a preparation CSV.
  const csv=(value:unknown)=>{let text=String(value??"");if(/^[\s]*[=+@-]/.test(text)&&!/^-[\d]+(?:\.\d+)?$/.test(text))text=`'${text}`;return `"${text.replaceAll('"','""')}"`;};
  const content="\uFEFF"+[
    ["ERPX ภ.พ.30 กระดาษทำการเพื่อให้คนตรวจและกรอกยื่น","THB",p.filing_month,"ไม่รวมเครดิตยกมา / ไม่ใช่ไฟล์นำเข้ากรมสรรพากร"],
    ["เอกสาร","วันที่เอกสาร","วันบัญชี","วันเกิด VAT","เดือนใช้สิทธิ","ประเภท","ฐานภาษี","VAT","ภาษีซื้อใช้สิทธิ","เหตุผลตรวจ"],
    ...snapshot.register.map((e:TaxRow)=>[e.document_reference,e.document_date,e.accounting_date,e.tax_point_date,e.claim_month,e.treatment,minorToDecimal(Number(e.taxable_minor)),minorToDecimal(Number(e.tax_minor)),e.direction==="input"?minorToDecimal(Number(e.deductible_minor)):"",e.review_reason]),
    ["ภาษีขาย",minorToDecimal(snapshot.result.outputVatMinor)],["ภาษีซื้อใช้สิทธิ",minorToDecimal(snapshot.result.deductibleInputMinor)],["ภาษีสุทธิก่อนเครดิตและปรับตามแบบ",minorToDecimal(snapshot.result.netMinor)],
  ].map(row=>row.map(csv).join(",")).join("\r\n")+"\r\n",exportHash=createHash("sha256").update(content).digest("hex");
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:"exported",version:input.expectedVersion+1,filename:`ERPX-PP30-workpaper-${p.filing_month}.csv`,content,exportHash},[
    assertion(write,key,"vat-package","EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND status='approved_for_export' AND version=?)",[id,actor.companyId,input.expectedVersion]),registerGuard(write,actor,key,p,r.json),
    ...reconciliationGuards(write,actor,key,String(p.chart_id),String(p.effective_date),String(currentProfile.effective_date),current.rows),
    write.prepare("UPDATE finance_statutory_packages SET status='exported',version=version+1,export_content=?,export_hash=?,updated_at=? WHERE id=?").bind(content,exportHash,Date.now(),id),
    packageEvent(write,actor,key,id,input.expectedVersion+1,"export",{exportHash,reason:input.reason}),
  ]);
}

export async function recordVatFiling(actor:WorkContext,id:string,raw:z.infer<typeof vatFilingSchema>,key:string){
  actor=companyFinance(actor,"finance.write");const input=vatFilingSchema.parse(raw),{write}=await holdingResources(actor.holdingId),operation="finance.vat-package.file";
  const replay=await replayCommand(write,actor,key,operation,id,input);if(replay)return replay;
  const p=await packageRow(write,actor,id);if(p.status!=="exported"||p.version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Record a filing receipt against the exported package.");
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:"filed",version:input.expectedVersion+1},[
    assertion(write,key,"vat-package","EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND status='exported' AND version=?)",[id,actor.companyId,input.expectedVersion]),
    write.prepare("UPDATE finance_statutory_packages SET status='filed',version=version+1,filing_reference=?,filed_at=?,evidence_json=(evidence_json::jsonb||?::jsonb)::text,updated_at=? WHERE id=?").bind(input.filingReference,Date.parse(`${input.filedDate}T00:00:00+07:00`),JSON.stringify({filing:input}),Date.now(),id),
    packageEvent(write,actor,key,id,input.expectedVersion+1,"file",input),
  ]);
}

export async function supersedeVatPackage(actor:WorkContext,id:string,raw:z.infer<typeof taxReviewSchema>,key:string){
  actor=companyFinance(actor,"finance.write");const input=taxReviewSchema.parse(raw),{write}=await holdingResources(actor.holdingId),operation="finance.vat-package.supersede";
  const replay=await replayCommand(write,actor,key,operation,id,input);if(replay)return replay;
  const p=await packageRow(write,actor,id);if(!["prepared","approved_for_export","exported"].includes(String(p.status))||p.version!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Only an unfiled package can be superseded. Filed returns require an additional return.");
  if(!samePerson(actor,p.prepared_by))actor=companyFinance(actor,"finance.approve");
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status:"superseded",version:input.expectedVersion+1},[
    assertion(write,key,"vat-package","EXISTS(SELECT 1 FROM finance_statutory_packages WHERE id=? AND company_id=? AND version=? AND status IN ('prepared','approved_for_export','exported'))",[id,actor.companyId,input.expectedVersion]),
    write.prepare("UPDATE finance_statutory_packages SET status='superseded',version=version+1,updated_at=? WHERE id=?").bind(Date.now(),id),packageEvent(write,actor,key,id,input.expectedVersion+1,"supersede",input),
  ]);
}

export async function vatWorkspace(actor:WorkContext,query:{chartId?:string;filingMonth?:string}={}){
  actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId);
  const profiles=(await write.prepare("SELECT * FROM finance_accounting_profiles WHERE company_id=? AND status='approved'").bind(actor.companyId).all<TaxRow>()).results;
  const chartId=query.chartId??String(profiles[0]?.statutory_chart_id??""),filingMonth=month.parse(query.filingMonth??new Date().toISOString().slice(0,7));
  if(chartId&&!await write.prepare("SELECT 1 FROM finance_charts WHERE id=? AND company_id=?").bind(chartId,actor.companyId).first())throw new ApiError(404,"CHART_NOT_FOUND","Ledger not found in this Company.");
  const dates=monthDates(filingMonth);
  const [entries,candidates,packages,corrections]=await Promise.all([
    write.prepare("SELECT * FROM finance_current_tax_entries WHERE company_id=? AND chart_id=? AND claim_month=? ORDER BY tax_point_date,id LIMIT 1000").bind(actor.companyId,chartId,filingMonth).all<TaxRow>(),
    write.prepare(`SELECT * FROM (SELECT i.id,i.version,i.invoice_number,i.invoice_date,i.kind,i.subtotal_minor,i.tax_minor,j.entry_date accounting_date,p.name party_name,'invoice' source_type
      FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id JOIN finance_parties p ON p.id=i.party_id
      WHERE i.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND i.currency='THB' AND i.tax_account_id IS NOT NULL
      AND NOT EXISTS(SELECT 1 FROM finance_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status<>'rejected')
      UNION ALL SELECT r.id,r.version,r.number,r.return_date,i.kind,-r.subtotal_minor,-r.tax_minor,j.entry_date,p.name,'commerce_return'
      FROM commerce_returns r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=r.credit_journal_id
      WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND i.currency='THB' AND i.tax_account_id IS NOT NULL
      AND NOT EXISTS(SELECT 1 FROM finance_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status<>'rejected')
      UNION ALL SELECT r.id,r.version,r.number,r.entry_date,i.kind,-r.subtotal_minor,-r.tax_minor,j.entry_date,p.name,'billing_credit'
      FROM finance_billing_corrections r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_parties p ON p.id=i.party_id JOIN finance_journals j ON j.id=r.credit_journal_id
      WHERE r.company_id=? AND j.chart_id=? AND r.status='posted' AND j.status='posted' AND i.currency='THB' AND i.tax_account_id IS NOT NULL
      AND NOT EXISTS(SELECT 1 FROM finance_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status<>'rejected')) candidates
      ORDER BY accounting_date,id LIMIT 100`).bind(actor.companyId,chartId,actor.companyId,chartId,actor.companyId,chartId).all<TaxRow>(),
    write.prepare("SELECT * FROM finance_statutory_packages WHERE company_id=? AND chart_id=? AND filing_month=? AND form_code='PP30' ORDER BY created_at DESC LIMIT 50").bind(actor.companyId,chartId,filingMonth).all<TaxRow>(),
    write.prepare("SELECT c.*,t.document_reference FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? ORDER BY (c.status='prepared') DESC,c.created_at DESC,c.id LIMIT 100").bind(actor.companyId,chartId).all<TaxRow>(),
  ]);
  const p=profiles.find(p=>p.statutory_chart_id===chartId&&String(p.effective_date)<=dates.start),reconciliation=p?await vatReconciliation(write,actor,chartId,dates.end,String(p.effective_date)):null;
  return {chartId,filingMonth,profiles,entries:entries.results,candidates:candidates.results,packages:packages.results,corrections:corrections.results,reconciliation};
}
export type VatCorrectionDetail={correction:TaxRow;entries:Array<{previous:TaxRow;replacement:TaxRow}>};
export async function vatCorrectionDetail(actor:WorkContext,id:string):Promise<VatCorrectionDetail>{
  actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId),correction=await correctionRow(write,actor,id);
  const entries=(await write.prepare("SELECT t.*,to_jsonb(p)::text previous_json FROM finance_tax_entries t JOIN finance_tax_entries p ON p.id=t.replaces_entry_id WHERE t.correction_id=? AND t.company_id=? ORDER BY (t.replaces_entry_id=?) DESC,t.id").bind(id,actor.companyId,correction.root_entry_id).all<TaxRow>()).results;
  return {correction,entries:entries.map(row=>{const {previous_json,...replacement}=row;return {previous:JSON.parse(String(previous_json)),replacement};})};
}
export async function taxEntryDetail(actor:WorkContext,id:string){
  actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId);
  const entry=await write.prepare("SELECT * FROM finance_tax_entries WHERE id=? AND company_id=?").bind(id,actor.companyId).first<TaxRow>();
  if(!entry)throw new ApiError(404,"TAX_ENTRY_NOT_FOUND","Tax entry not found in this Company.");
  const [history,current,source]=await Promise.all([
    write.prepare("SELECT * FROM finance_tax_entries WHERE company_id=? AND source_type=? AND source_id=? ORDER BY created_at,id LIMIT 1000").bind(actor.companyId,entry.source_type,entry.source_id).all<TaxRow>(),
    write.prepare("SELECT id FROM finance_current_tax_entries WHERE company_id=? AND source_type=? AND source_id=? AND status='approved'").bind(actor.companyId,entry.source_type,entry.source_id).first<TaxRow>(),
    write.prepare(`SELECT version FROM ${entry.source_type==="invoice"?"finance_invoices":entry.source_type==="billing_credit"?"finance_billing_corrections":"commerce_returns"} WHERE id=? AND company_id=?`).bind(entry.source_id,actor.companyId).first<TaxRow>(),
  ]);
  return {entry,history:history.results,currentId:current?.id??null,currentSourceVersion:source?.version??null};
}
export async function vatPackageDetail(actor:WorkContext,id:string){
  actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId),p=await packageRow(write,actor,id);
  const [events,currentJson,pendingCorrections]=await Promise.all([
    write.prepare("SELECT * FROM finance_statutory_package_events WHERE package_id=? ORDER BY version").bind(id).all<TaxRow>(),
    write.prepare(`SELECT ${registerJson} value`).bind(actor.companyId,p.chart_id,p.filing_month).first<string>("value"),
    write.prepare(pendingCorrectionsSql).bind(actor.companyId,p.chart_id,monthDates(String(p.filing_month)).end).first<number>("count"),
  ]);
  return {package:p,snapshot:JSON.parse(String(p.snapshot_json)),events:events.results,currentRegisterMatches:digest(JSON.parse(String(currentJson)))===p.source_hash,pendingCorrections:Number(pendingCorrections??0)};
}
export async function vatPackageExport(actor:WorkContext,id:string){
  actor=requirePermission(actor,"finance.read");const {write}=await holdingResources(actor.holdingId),p=await packageRow(write,actor,id);
  if(!p.export_content||!p.export_hash)throw new ApiError(404,"VAT_EXPORT_NOT_FOUND","This package has no retained export yet.");
  if(createHash("sha256").update(String(p.export_content)).digest("hex")!==p.export_hash)throw new ApiError(409,"VAT_EXPORT_INTEGRITY","The retained export does not match its recorded hash.");
  return {content:String(p.export_content),exportHash:String(p.export_hash),filename:`ERPX-PP30-workpaper-${p.filing_month}.csv`};
}
