import "server-only";
import { z } from "zod";
import { samePerson } from "@/lib/iam";
import { decimalToMinor, minorToDecimal } from "@/lib/finance-money";
import { thaiFinanceTaxPack, thaiWithholdingAmount, type ThaiWithholdingCategory } from "@/lib/thai-finance-tax-pack";
import type { WorkContext } from "./authorization";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { currentInvoiceLines, type PersistedInvoiceLine } from "./finance-invoice-lines";
import { requireCompanyTaxAuthority, reviewedTaxIdentity, taxIdentityGuards, taxSourceDigest, type TaxIdentity } from "./finance-tax-identities";
import { prepareThaiDocumentIssue } from "./finance-thai-documents";

type Row = Record<string,string|number|null>;
type WorkspaceInvoice = Row & { id:string;invoice_number:string;subtotal_minor:number;tax_minor:number;total_minor:number;outstanding_minor:number;line_revision:number };
const amount=z.string().regex(/^\d+(?:\.\d{1,2})?$/),evidence=z.string().trim().min(10).max(2000);
const categories=Object.keys(thaiFinanceTaxPack.rates) as [ThaiWithholdingCategory,...ThaiWithholdingCategory[]];
export const withholdingAssessmentSchema=z.object({
 expectedVersion:z.number().int().positive(),payerIdentityId:z.string().min(1),payeeIdentityId:z.string().min(1),liabilityAccountId:z.string().min(1),
 filingMode:z.literal("ordinary").default("ordinary"),rulePackVersion:z.literal(thaiFinanceTaxPack.version),classificationEvidence:evidence,
 allocations:z.array(z.object({invoiceId:z.string().min(1),amount})).min(1).max(100),
 lines:z.array(z.object({invoiceId:z.string().min(1),sourceLineId:z.string().min(1),category:z.enum(categories),contractRef:z.string().trim().min(2).max(200),contractTotal:amount})).min(1).max(1000),
});
export const withholdingDecisionSchema=z.object({expectedVersion:z.number().int().positive(),action:z.enum(["approve","reject","cancel"]),reason:evidence});
type Allocation={invoiceId:string;amount:string};
type CapturedLine={invoiceId:string;invoiceVersion:number;lineRevision:number;sourceLineId:string;description:string;category:ThaiWithholdingCategory;contractRef:string;contractTotalMinor:number;allocatedGrossMinor:number;priorPaidGrossMinor:number;originalIncomeMinor:number;incomeMinor:number;priorWithheldMinor:number;rateBps:number;taxMinor:number;source:unknown};
type Source={paymentId:string;paymentDate:string;grossMinor:number;currency:string;partyId:string;bankAccountId:string;bankAccountVersion:number;chartId:string;payerIdentityId:string;payerIdentityVersion:number;payeeIdentityId:string;payeeIdentityVersion:number;liabilityAccountId:string;liabilityAccountVersion:number;allocations:Allocation[];lines:CapturedLine[];rulePackVersion:string;rulePackDigest:string};
type Assessment=Row&{id:string;payment_id:string;status:string;source_snapshot_json:string;source_fingerprint:string;version:number;tax_minor:number;income_minor:number;gross_minor:number};

function safe(value:unknown){const number=Number(value);if(!Number.isSafeInteger(number)||number<0)throw new ApiError(422,"WITHHOLDING_AMOUNT_INVALID","The source amount must use exact nonnegative Thai-baht minor units.");return number;}
function portion(amountMinor:number,paidGross:number,gross:number){return Number((BigInt(amountMinor)*BigInt(paidGross)+BigInt(Math.floor(gross/2)))/BigInt(gross));}
function inPack(date:string){if(date<thaiFinanceTaxPack.effectiveFrom||date>thaiFinanceTaxPack.effectiveTo)throw new ApiError(422,"WITHHOLDING_RULES_NOT_READY","The ordinary Thai withholding rule pack must cover the actual payment date.");}

/** Generic gross-bank adapters cannot bypass a configured statutory owner. */
export async function assertControlledThaiTaxPaymentOwner(db:SqlDatabase,actor:WorkContext,paymentDate:string){
 const configured=await db.prepare("SELECT 1 FROM finance_tax_identities WHERE company_id=? AND party_id IS NULL AND status='approved' AND effective_from<=? AND effective_to>=?").bind(actor.companyId,paymentDate,paymentDate).first();
 if(configured)throw new ApiError(409,"WITHHOLDING_OWNER_REQUIRED","This Company has reviewed Thai statutory payer evidence. Prepare and independently review the supplier payment's income classification in the Finance controlled payment workflow before its net bank result.");
}

async function captureSource(db:SqlDatabase,actor:WorkContext,payment:Row,input:z.infer<typeof withholdingAssessmentSchema>,commandId:string):Promise<{source:Source;payer:TaxIdentity;payee:TaxIdentity;incomeMinor:number;taxMinor:number;guards:SqlStatement[]}>{
 const date=String(payment.payment_date);inPack(date);
 if(payment.kind!=="payment"||payment.currency!=="THB"||!payment.party_id||!payment.bank_account)throw new ApiError(422,"WITHHOLDING_ADAPTER_UNSUPPORTED","Use the ordinary domestic THB supplier payment adapter; other income, foreign payees and provider filing require their owner adapter.");
 const payer=await reviewedTaxIdentity(db,actor,input.payerIdentityId,null,date),payee=await reviewedTaxIdentity(db,actor,input.payeeIdentityId,String(payment.party_id),date);
 const bank=await db.prepare("SELECT a.id,a.version,a.chart_id,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.status='active' AND a.control_kind='bank' AND a.currency='THB'").bind(payment.bank_account,actor.companyId).first<Row>();
 const liability=await db.prepare("SELECT id,version,chart_id FROM finance_accounts WHERE id=? AND company_id=? AND category='liability' AND control_kind='none' AND status='active' AND currency='THB'").bind(input.liabilityAccountId,actor.companyId).first<Row>();
 if(!bank||bank.base_currency!=="THB"||!liability||liability.chart_id!==bank.chart_id||liability.id===bank.id)throw new ApiError(422,"WITHHOLDING_LEDGER_INVALID","Select a current THB withholding liability on the payment's functional THB ledger.");
 const ids=input.allocations.map(item=>item.invoiceId),gross=safe(payment.amount_minor),allocations=input.allocations.map(item=>({...item,amount:minorToDecimal(decimalToMinor(item.amount))}));
 if(new Set(ids).size!==ids.length||allocations.some(item=>decimalToMinor(item.amount)<=0)||allocations.reduce((sum,item)=>sum+decimalToMinor(item.amount),0)!==gross)throw new ApiError(422,"WITHHOLDING_ALLOCATION_MISMATCH","Allocate the complete gross supplier payment once across recognized invoices.");
 if(new Set(input.lines.map(line=>`${line.invoiceId}:${line.sourceLineId}`)).size!==input.lines.length)throw new ApiError(422,"WITHHOLDING_LINE_DUPLICATE","Classify each immutable source line exactly once.");
 const lines:CapturedLine[]=[],guards:SqlStatement[]=[assertion(db,commandId,"withholding-bank-account","EXISTS(SELECT 1 FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.version=? AND a.status='active' AND a.control_kind='bank' AND a.currency='THB' AND c.base_currency='THB')",[bank.id,actor.companyId,bank.version]),assertion(db,commandId,"withholding-liability-account","EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=? AND status='active' AND category='liability' AND control_kind='none' AND currency='THB')",[liability.id,actor.companyId,liability.version])];
 for(const allocation of allocations){
  const invoice=await db.prepare("SELECT i.*,j.chart_id,j.status journal_status,j.version journal_version FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?").bind(allocation.invoiceId,actor.companyId).first<Row>(),paid=decimalToMinor(allocation.amount);
  if(!invoice||invoice.kind!=="payable"||invoice.party_id!==payment.party_id||invoice.currency!=="THB"||invoice.chart_id!==bank.chart_id||!["posted","partially_paid"].includes(String(invoice.status))||invoice.journal_status!=="posted"||invoice.source_type==="expense_claim"||safe(invoice.outstanding_minor)<paid||safe(invoice.credit_total_minor??0)>0||Number(invoice.fx_rate_micros)!==1_000_000)throw new ApiError(422,"WITHHOLDING_INVOICE_INVALID","The gross payment must settle an open, recognized domestic supplier invoice without a pending owner credit adjustment.");
  const total=safe(invoice.total_minor),priorPaid=total-safe(invoice.outstanding_minor);
  const previous=await db.prepare("SELECT l.*,a.payment_id FROM finance_withholding_income_lines l JOIN finance_withholding_assessments a ON a.id=l.assessment_id JOIN finance_payments p ON p.id=a.payment_id WHERE l.invoice_id=? AND l.company_id=? AND a.status='consumed' AND p.status='settled' ORDER BY l.assessment_id,l.ordinal").bind(allocation.invoiceId,actor.companyId).all<Row>();
  const priorGross=new Map<string,number>();for(const line of previous.results)priorGross.set(String(line.assessment_id),safe(line.allocated_gross_minor));
  if([...priorGross.values()].reduce((sum,n)=>sum+n,0)!==priorPaid)throw new ApiError(409,"WITHHOLDING_HISTORY_NOT_RECONCILED","Reconcile prior supplier payments and their reviewed tax receipts before calculating a later installment.");
  let sourceLines=await currentInvoiceLines(db,actor.companyId,allocation.invoiceId,Number(invoice.line_revision));
  if(!sourceLines.length)sourceLines=[{id:`${allocation.invoiceId}:header:${invoice.line_revision}`,description:String(invoice.invoice_number),subtotal_minor:safe(invoice.subtotal_minor),tax_minor:safe(invoice.tax_minor),source_snapshot_json:JSON.stringify({invoiceId:invoice.id,invoiceVersion:invoice.version,journalId:invoice.journal_id}),source_version:Number(invoice.line_revision)}] as typeof sourceLines;
  if(!sourceLines.length||sourceLines.some(line=>safe(line.subtotal_minor)<=0))throw new ApiError(422,"WITHHOLDING_SOURCE_LINES_INVALID","Classify positive immutable income lines from the recognized source.");
  const classifications=input.lines.filter(line=>line.invoiceId===allocation.invoiceId);
  if(classifications.length!==sourceLines.length||classifications.some(line=>!sourceLines.some(source=>source.id===line.sourceLineId)))throw new ApiError(422,"WITHHOLDING_CLASSIFICATION_INCOMPLETE","Every immutable source line needs its actual income category and contract evidence.");
  for(const [ordinal,line] of sourceLines.entries()){
   const classification=classifications.find(item=>item.sourceLineId===line.id)!,originalIncome=safe(line.subtotal_minor),contract=decimalToMinor(classification.contractTotal);
   if(contract<originalIncome||(classification.category==="non_life_insurance"&&payee.entity_kind!=="domestic_company"))throw new ApiError(422,"WITHHOLDING_CONTRACT_INVALID","The reviewed contract must cover the original income; insurance premiums require the supported domestic corporate payee.");
   const past=previous.results.filter(item=>item.source_line_id===line.id);
   if(past.some(item=>item.category!==classification.category||item.contract_ref!==classification.contractRef||safe(item.contract_total_minor)!==contract))throw new ApiError(409,"WITHHOLDING_CLASSIFICATION_CHANGED","Later installments must retain the independently reviewed original income classification and contract.");
   const priorIncome=past.reduce((sum,item)=>sum+safe(item.income_minor),0),priorTax=past.reduce((sum,item)=>sum+safe(item.tax_minor),0),cumulative=portion(originalIncome,priorPaid+paid,total),expectedPrior=portion(originalIncome,priorPaid,total);
   if(priorIncome!==expectedPrior)throw new ApiError(409,"WITHHOLDING_HISTORY_NOT_RECONCILED","Prior tax receipts must reconcile to the original invoice income carrying amount.");
   const calculation=thaiWithholdingAmount(cumulative,classification.category,contract),tax=calculation.taxMinor-priorTax;
   if(tax<0||past.some(item=>Number(item.rate_bps)!==calculation.rateBps))throw new ApiError(409,"WITHHOLDING_HISTORY_NOT_RECONCILED","Retain the original ordinary rate and reconcile prior withheld tax before another installment.");
   lines.push({invoiceId:allocation.invoiceId,invoiceVersion:Number(invoice.version),lineRevision:Number(invoice.line_revision),sourceLineId:String(line.id),description:String(line.description),category:classification.category,contractRef:classification.contractRef,contractTotalMinor:contract,allocatedGrossMinor:paid,priorPaidGrossMinor:priorPaid,originalIncomeMinor:originalIncome,incomeMinor:cumulative-priorIncome,priorWithheldMinor:priorTax,rateBps:calculation.rateBps,taxMinor:tax,source:{invoiceNumber:invoice.invoice_number,journalId:invoice.journal_id,journalVersion:invoice.journal_version,ordinal,originalLine:JSON.parse(line.source_snapshot_json)}});
  }
  guards.push(assertion(db,commandId,`invoice-${allocation.invoiceId}`,"EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.outstanding_minor=? AND i.status IN ('posted','partially_paid') AND j.status='posted')",[allocation.invoiceId,actor.companyId,invoice.version,invoice.outstanding_minor]),assertion(db,commandId,`supplier-${allocation.invoiceId}`,"NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown'))",[allocation.invoiceId]),assertion(db,commandId,`credit-${allocation.invoiceId}`,"NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')",[allocation.invoiceId]));
 }
 if(input.lines.some(line=>!ids.includes(line.invoiceId)))throw new ApiError(422,"WITHHOLDING_CLASSIFICATION_INCOMPLETE","Classification lines must belong to these exact invoice allocations.");
 const source:Source={paymentId:String(payment.id),paymentDate:date,grossMinor:gross,currency:"THB",partyId:String(payment.party_id),bankAccountId:String(bank.id),bankAccountVersion:Number(bank.version),chartId:String(bank.chart_id),payerIdentityId:payer.id,payerIdentityVersion:Number(payer.version),payeeIdentityId:payee.id,payeeIdentityVersion:Number(payee.version),liabilityAccountId:String(liability.id),liabilityAccountVersion:Number(liability.version),allocations,lines,rulePackVersion:thaiFinanceTaxPack.version,rulePackDigest:taxSourceDigest(thaiFinanceTaxPack)};
 const incomeMinor=lines.reduce((sum,line)=>sum+line.incomeMinor,0),taxMinor=lines.reduce((sum,line)=>sum+line.taxMinor,0);
 if(!Number.isSafeInteger(incomeMinor)||incomeMinor<=0||!Number.isSafeInteger(taxMinor)||taxMinor>=gross)throw new ApiError(422,"WITHHOLDING_AMOUNT_INVALID","The recognized income and tax must fit the gross supplier payment.");
 return {source,payer,payee,incomeMinor,taxMinor,guards};
}

function sourceInput(source:Source,expectedVersion:number,classificationEvidence:string):z.infer<typeof withholdingAssessmentSchema>{return {expectedVersion,payerIdentityId:source.payerIdentityId,payeeIdentityId:source.payeeIdentityId,liabilityAccountId:source.liabilityAccountId,filingMode:"ordinary",rulePackVersion:thaiFinanceTaxPack.version,classificationEvidence,allocations:source.allocations,lines:source.lines.map(line=>({invoiceId:line.invoiceId,sourceLineId:line.sourceLineId,category:line.category,contractRef:line.contractRef,contractTotal:minorToDecimal(line.contractTotalMinor)}))};}

export async function prepareWithholdingAssessment(actor:WorkContext,paymentId:string,input:z.infer<typeof withholdingAssessmentSchema>,commandId:string){
 actor=requireCompanyTaxAuthority(actor,"finance.write");input=withholdingAssessmentSchema.parse(input);
 const {write}=await holdingResources(actor.holdingId),id=`withholding-${commandId}`,operation="finance.withholding.prepare",previous=await replayCommand(write,actor,commandId,operation,id,input);if(previous)return previous;
 const payment=await write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=?").bind(paymentId,actor.companyId).first<Row>();if(!payment)throw new ApiError(404,"PAYMENT_NOT_FOUND","Select a Company supplier payment.");
 if(payment.status!=="draft"||Number(payment.version)!==input.expectedVersion||!samePerson(actor,payment.created_by))throw new ApiError(409,"WITHHOLDING_PAYMENT_CHANGED","The payment preparer must classify its current draft before independent approval.");
 const captured=await captureSource(write,actor,payment,input,commandId),now=Date.now();
 return executeCommand(write,actor,commandId,operation,input,id,1,{id,status:"draft",version:1,incomeMinor:captured.incomeMinor,taxMinor:captured.taxMinor,netBankMinor:Number(payment.amount_minor)-captured.taxMinor},[
  assertion(write,commandId,"payment","EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='draft' AND created_by=?)",[paymentId,actor.companyId,input.expectedVersion,actor.userId]),
  assertion(write,commandId,"one-assessment","NOT EXISTS(SELECT 1 FROM finance_withholding_assessments WHERE payment_id=? AND status IN ('draft','approved','consumed'))",[paymentId]),
  ...taxIdentityGuards(write,actor,commandId,[captured.payer,captured.payee]),...captured.guards,
  write.prepare("INSERT INTO finance_withholding_assessments(id,company_id,payment_id,payment_version,payer_identity_id,payer_identity_version,payee_identity_id,payee_identity_version,liability_account_id,liability_account_version,payment_date,form_kind,gross_minor,income_minor,tax_minor,source_fingerprint,source_snapshot_json,rule_pack_version,rule_pack_digest,classification_evidence,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,paymentId,input.expectedVersion,captured.payer.id,captured.payer.version,captured.payee.id,captured.payee.version,captured.source.liabilityAccountId,captured.source.liabilityAccountVersion,payment.payment_date,captured.payee.entity_kind==="resident_individual"?"PND3":"PND53",payment.amount_minor,captured.incomeMinor,captured.taxMinor,taxSourceDigest(captured.source),JSON.stringify(captured.source),thaiFinanceTaxPack.version,taxSourceDigest(thaiFinanceTaxPack),input.classificationEvidence,actor.userId,now,now),
  ...captured.source.lines.map((line,ordinal)=>write.prepare("INSERT INTO finance_withholding_income_lines(id,assessment_id,company_id,invoice_id,invoice_version,invoice_line_revision,source_line_id,description,category,contract_ref,contract_total_minor,allocated_gross_minor,prior_paid_gross_minor,original_income_minor,income_minor,prior_withheld_minor,rate_bps,tax_minor,source_snapshot_json,ordinal,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(`${id}:line:${ordinal}`,id,actor.companyId,line.invoiceId,line.invoiceVersion,line.lineRevision,line.sourceLineId,line.description,line.category,line.contractRef,line.contractTotalMinor,line.allocatedGrossMinor,line.priorPaidGrossMinor,line.originalIncomeMinor,line.incomeMinor,line.priorWithheldMinor,line.rateBps,line.taxMinor,JSON.stringify(line.source),ordinal,now)),
 ]);
}

export async function decideWithholdingAssessment(actor:WorkContext,id:string,input:z.infer<typeof withholdingDecisionSchema>,commandId:string){
 actor=requireCompanyTaxAuthority(actor,input.action==="cancel"?"finance.write":"finance.approve");input=withholdingDecisionSchema.parse(input);
 const {write}=await holdingResources(actor.holdingId),operation=`finance.withholding.${input.action}`,previous=await replayCommand(write,actor,commandId,operation,id,input);if(previous)return previous;
 const assessment=await write.prepare("SELECT * FROM finance_withholding_assessments WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Assessment>();if(!assessment)throw new ApiError(404,"WITHHOLDING_NOT_FOUND","The Company withholding assessment was not found.");
 if(assessment.status!=="draft"||Number(assessment.version)!==input.expectedVersion)throw new ApiError(409,"WITHHOLDING_CHANGED","Refresh the current pending assessment.");
 if(input.action==="cancel"?!samePerson(actor,assessment.prepared_by):samePerson(actor,assessment.prepared_by))throw new ApiError(403,"INDEPENDENT_TAX_REVIEW_REQUIRED","Only the preparer may cancel; a different Finance approver must review the tax classification.");
 const guards:SqlStatement[]=[assertion(write,commandId,"assessment","EXISTS(SELECT 1 FROM finance_withholding_assessments WHERE id=? AND company_id=? AND status='draft' AND version=?)",[id,actor.companyId,input.expectedVersion])],now=Date.now();
 if(input.action==="approve"){
  const payment=await write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=?").bind(assessment.payment_id,actor.companyId).first<Row>(),source=JSON.parse(assessment.source_snapshot_json) as Source;
  if(!payment||payment.status!=="draft"||Number(payment.version)!==Number(assessment.payment_version))throw new ApiError(409,"WITHHOLDING_PAYMENT_CHANGED","Prepare a new assessment against the current payment draft.");
  const current=await captureSource(write,actor,payment,sourceInput(source,Number(payment.version),String(assessment.classification_evidence)),commandId);
  if(taxSourceDigest(current.source)!==assessment.source_fingerprint||taxSourceDigest(source)!==assessment.source_fingerprint)throw new ApiError(409,"WITHHOLDING_SOURCE_CHANGED","An invoice, reviewed legal identity or ledger changed; cancel and prepare a current assessment.");
  guards.push(...current.guards,...taxIdentityGuards(write,actor,commandId,[current.payer,current.payee]),assertion(write,commandId,"payment","EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND status='draft' AND version=?)",[payment.id,actor.companyId,payment.version]),write.prepare("UPDATE finance_payments SET withholding_assessment_id=?,withheld_minor=?,version=version+1,updated_at=? WHERE id=?").bind(id,assessment.tax_minor,now,payment.id));
 }
 const status=input.action==="approve"?"approved":input.action==="reject"?"rejected":"cancelled";
 return executeCommand(write,actor,commandId,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},[...guards,write.prepare("UPDATE finance_withholding_assessments SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1,updated_at=? WHERE id=?").bind(status,actor.userId,input.reason,now,now,id)]);
}

/** Optional only until a Company has installed a reviewed ordinary tax payer; never guesses a tax rate. */
export async function readyPaymentWithholding(db:SqlDatabase,actor:WorkContext,payment:Row,commandId:string,actualDate?:string,allocations?:Allocation[]){
 if(payment.kind!=="payment")return null;
 const date=actualDate??String(payment.payment_date),configured=await db.prepare("SELECT 1 FROM finance_tax_identities WHERE company_id=? AND party_id IS NULL AND status='approved' AND effective_from<=? AND effective_to>=? AND EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND person_id IS NULL)").bind(actor.companyId,date,date,payment.party_id,actor.companyId).first();
 if(!payment.withholding_assessment_id){if(configured)throw new ApiError(422,"WITHHOLDING_REVIEW_REQUIRED","Classify all invoice income with the Company's reviewed ordinary tax identities before payment approval.");return null;}
 const assessment=await db.prepare("SELECT * FROM finance_withholding_assessments WHERE id=? AND company_id=? AND payment_id=? AND status='approved'").bind(payment.withholding_assessment_id,actor.companyId,payment.id).first<Assessment>();
 if(!assessment)throw new ApiError(409,"WITHHOLDING_NOT_READY","The exact independently approved assessment must remain current through bank confirmation.");
 const source=JSON.parse(assessment.source_snapshot_json) as Source,current=await captureSource(db,actor,payment,sourceInput(source,Number(payment.version),String(assessment.classification_evidence)),commandId);
 if(taxSourceDigest(source)!==assessment.source_fingerprint||taxSourceDigest(current.source)!==assessment.source_fingerprint||Number(payment.withheld_minor)!==safe(assessment.tax_minor)||Number(payment.amount_minor)!==safe(assessment.gross_minor))throw new ApiError(409,"WITHHOLDING_SOURCE_CHANGED","Re-review the changed source before the payment can be released or settled.");
 if(allocations){const normalized=allocations.map(item=>({...item,amount:minorToDecimal(decimalToMinor(item.amount))}));if(taxSourceDigest(normalized)!==taxSourceDigest(source.allocations))throw new ApiError(422,"WITHHOLDING_ALLOCATION_CHANGED","Confirm the exact gross invoice allocations independently reviewed for this withholding assessment.");}
 if(actualDate){inPack(actualDate);await reviewedTaxIdentity(db,actor,current.payer.id,null,actualDate);await reviewedTaxIdentity(db,actor,current.payee.id,String(payment.party_id),actualDate);}
 return {assessment,...current,guards:[...current.guards,...taxIdentityGuards(db,actor,commandId,[current.payer,current.payee],"withholding-identity"),assertion(db,commandId,"approved-withholding","EXISTS(SELECT 1 FROM finance_withholding_assessments WHERE id=? AND company_id=? AND status='approved' AND version=? AND source_fingerprint=?)",[assessment.id,actor.companyId,assessment.version,assessment.source_fingerprint])]};
}

export async function prepareWithholdingConsumption(db:SqlDatabase,actor:WorkContext,ready:NonNullable<Awaited<ReturnType<typeof readyPaymentWithholding>>>,input:{commandId:string;paymentId:string;bankTransactionId:string;bankDate:string;journalId:string}){
 const {assessment,source,payer,payee}=ready,now=Date.now();
 const issue=Number(assessment.tax_minor)===0?null:await prepareThaiDocumentIssue(db,actor,{payer,payee,paymentId:input.paymentId,assessmentId:assessment.id,commandId:input.commandId,snapshot:{schema:"finance.thai.document.v1",kind:"withholding_certificate",issueDate:input.bankDate,paymentDate:input.bankDate,payer,payee,formKind:String(assessment.form_kind),incomeMinor:source.lines.filter(line=>line.rateBps>0).reduce((sum,line)=>sum+line.incomeMinor,0),taxMinor:Number(assessment.tax_minor),grossMinor:Number(assessment.gross_minor),items:source.lines.filter(line=>line.rateBps>0).map(line=>({description:thaiFinanceTaxPack.rates[line.category].label+": "+line.description,incomeMinor:line.incomeMinor,taxMinor:line.taxMinor,section:thaiFinanceTaxPack.rates[line.category].section,category:line.category,rateBps:line.rateBps})),source:{paymentId:input.paymentId,assessmentId:assessment.id,assessmentVersion:assessment.version,assessmentFingerprint:assessment.source_fingerprint,bankTransactionId:input.bankTransactionId,journalId:input.journalId,allocations:source.allocations,lineReceipts:source.lines,payerVersion:payer.version,payeeVersion:payee.version,rulePackVersion:thaiFinanceTaxPack.version,rulePackDigest:taxSourceDigest(thaiFinanceTaxPack)},edition:1,reason:"Issued immediately with confirmed bank payment and independently reviewed ordinary withholding."}});
 return {documentId:issue?.id??null,steps:[db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`finance-withholding-month:${actor.companyId}:${input.bankDate.slice(0,7)}:${assessment.form_kind}`),writeConsumed(db,assessment.id,now),...(issue?.steps??[])]};
}
function writeConsumed(db:SqlDatabase,id:string,now:number){return db.prepare("UPDATE finance_withholding_assessments SET status='consumed',consumed_at=?,version=version+1,updated_at=? WHERE id=?").bind(now,now,id);}

export async function prepareWithholdingReversal(db:SqlDatabase,actor:WorkContext,payment:Row,commandId:string,reversalJournalId:string,reason:string){
 if(!payment.withholding_assessment_id)return [];
 const assessment=await db.prepare("SELECT id,version,form_kind FROM finance_withholding_assessments WHERE id=? AND company_id=? AND status='consumed'").bind(payment.withholding_assessment_id,actor.companyId).first<Row>();if(!assessment)throw new ApiError(409,"WITHHOLDING_REVERSAL_NOT_READY","The payment's consumed tax receipt must be available for its financial reversal.");
 const remitted=await db.prepare("SELECT 1 FROM finance_withholding_remittance_items i JOIN finance_withholding_remittances r ON r.id=i.remittance_id WHERE i.assessment_id=? AND r.status='confirmed'").bind(assessment.id).first();if(remitted)throw new ApiError(409,"WITHHOLDING_REMITTANCE_RECOVERY_REQUIRED","Record the actual tax bank recovery before reversing a remitted supplier withholding source.");
 const docs=await db.prepare("SELECT id FROM finance_issued_thai_documents WHERE payment_id=? AND company_id=? AND document_kind='withholding_certificate'").bind(payment.id,actor.companyId).all<{id:string}>(),now=Date.now();
 return [db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`finance-withholding-month:${actor.companyId}:${String(payment.journal_entry_date??payment.payment_date).slice(0,7)}:${assessment.form_kind}`),assertion(db,commandId,"withholding-not-remitted","NOT EXISTS(SELECT 1 FROM finance_withholding_remittance_items i JOIN finance_withholding_remittances r ON r.id=i.remittance_id WHERE i.assessment_id=? AND r.status='confirmed')",[assessment.id]),assertion(db,commandId,"withholding-reversal","EXISTS(SELECT 1 FROM finance_withholding_assessments WHERE id=? AND company_id=? AND status='consumed' AND version=?)",[assessment.id,actor.companyId,assessment.version]),db.prepare("UPDATE finance_withholding_assessments SET status='reversed',reversal_journal_id=?,version=version+1,updated_at=? WHERE id=?").bind(reversalJournalId,now,assessment.id),...docs.results.map((doc,index)=>db.prepare("INSERT INTO finance_thai_document_events(id,document_id,company_id,action,reason,actor_id,command_id,created_at) VALUES(?,?,?,'cancelled',?,?,?,?) ON CONFLICT(document_id,action) DO NOTHING").bind(`thai-cancel-${commandId}-${index}`,doc.id,actor.companyId,reason,actor.userId,commandId,now))];
}

export async function withholdingWorkspace(actor:WorkContext,paymentId?:string){
 actor=requireCompanyTaxAuthority(actor,"finance.read");const {write}=await holdingResources(actor.holdingId);
 const [identities,assessments,documents,accounts,payment]=await Promise.all([
  write.prepare("SELECT id,party_id,legal_name,tax_id,status,version FROM finance_tax_identities WHERE company_id=? AND status='approved' ORDER BY legal_name").bind(actor.companyId).all(),
  write.prepare(`SELECT a.*,p.payment_number,p.status payment_status FROM finance_withholding_assessments a JOIN finance_payments p ON p.id=a.payment_id WHERE a.company_id=? ${paymentId?"AND a.payment_id=?":""} ORDER BY a.created_at DESC LIMIT 100`).bind(actor.companyId,...(paymentId?[paymentId]:[])).all(),
  write.prepare(`SELECT id,document_number,document_kind,payment_id,invoice_id,issue_date,edition,previous_document_id,export_sha256 FROM finance_issued_thai_documents WHERE company_id=? ${paymentId?"AND payment_id=?":""} ORDER BY created_at DESC LIMIT 100`).bind(actor.companyId,...(paymentId?[paymentId]:[])).all(),
  write.prepare("SELECT a.id,a.code,a.name,a.chart_id FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND a.category='liability' AND a.control_kind='none' AND a.currency='THB' AND c.base_currency='THB' ORDER BY a.code").bind(actor.companyId).all(),
  paymentId?write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=?").bind(paymentId,actor.companyId).first<Row>():null,
 ]);
 const invoices=payment?await write.prepare("SELECT i.id,i.invoice_number,i.subtotal_minor,i.tax_minor,i.total_minor,i.outstanding_minor,i.line_revision FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.company_id=? AND i.party_id=? AND i.kind='payable' AND i.currency='THB' AND i.status IN ('posted','partially_paid') AND j.status='posted' ORDER BY i.due_date,i.id").bind(actor.companyId,payment.party_id).all<WorkspaceInvoice>():{results:[] as WorkspaceInvoice[]};
 const sources=await Promise.all(invoices.results.map(async invoice=>{
  const current=await currentInvoiceLines(write,actor.companyId,invoice.id,Number(invoice.line_revision));
  const lines:Pick<PersistedInvoiceLine,"id"|"description"|"subtotal_minor"|"tax_minor">[]=current.length?current:[{id:`${invoice.id}:header:${invoice.line_revision}`,description:invoice.invoice_number,subtotal_minor:Number(invoice.subtotal_minor),tax_minor:Number(invoice.tax_minor)}];
  return {...invoice,lines};
 }));
 return {identities:identities.results,assessments:assessments.results,documents:documents.results,accounts:accounts.results,payment,invoices:sources,rulePack:thaiFinanceTaxPack};
}
