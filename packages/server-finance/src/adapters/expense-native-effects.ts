import "server-only";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {snapshotMoney,snapshotMoneyBatch,snapshotMoneyReversal} from '@erpx/server-finance/adapters/company-money';
import {requireFinanceMoneyScale} from '@erpx/server-finance/adapters/finance-money-policy';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Expense consumes owner effects on its supplied transaction; this port only prepares the original statements in their original order. */
export function expenseNativeEffects(db:SqlDatabase){return {
/** The original Expense path inserts its journal and payable already posted,
 * so it must prepare the same canonical Money evidence as ordinary Finance
 * journal/invoice posting. No old row, result or receipt is rewritten. */
preparePostedMoney:async(actor:WorkContext,facts:{journalId:string;invoiceId:string;transactionDate:string;currency:string;totalMinor:number})=>{
 await requireFinanceMoneyScale(db,actor.companyId,facts.currency);
 const common={transactionDate:facts.transactionDate,currency:facts.currency,currencyScale:2,amountUnits:String(facts.totalMinor)};
 const prepared=await snapshotMoneyBatch(db,actor,[
  {...common,sourceType:'finance_journal',sourceId:facts.journalId,amountKind:'debit_total'},
  {...common,sourceType:'finance_invoice',sourceId:facts.invoiceId,amountKind:'total'},
 ]);
 return prepared.statements;
},
/** A new reversal preserves captured original evidence. For a pre-capture
 * journal, quote only the new reversal; never manufacture historical evidence. */
prepareReversalMoney:async(actor:WorkContext,facts:{journalId:string;originalJournalId:string;transactionDate:string;currency:string;totalMinor:number})=>{
 const original={sourceType:'finance_journal',sourceId:facts.originalJournalId,amountKind:'debit_total'};
 const captured=await db.prepare('SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND source_type=? AND source_id=? AND amount_kind=?').bind(actor.companyId,original.sourceType,original.sourceId,original.amountKind).first();
 if(captured)return (await snapshotMoneyReversal(db,actor,{sourceType:'finance_journal',sourceId:facts.journalId,amountKind:'debit_total',transactionDate:facts.transactionDate,amountUnits:String(facts.totalMinor),original})).statement;
 await requireFinanceMoneyScale(db,actor.companyId,facts.currency);
 return (await snapshotMoney(db,actor,{sourceType:'finance_journal',sourceId:facts.journalId,amountKind:'debit_total',transactionDate:facts.transactionDate,currency:facts.currency,currencyScale:2,amountUnits:String(-facts.totalMinor)})).statement;
},
retainEmployeeParty:(facts:{id:string;companyId:string;code:string;name:string|number|null;currency:string|number|null;personId:string|number|null;createdAt:number;updatedAt:number})=>db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,person_id,created_at,updated_at) VALUES(?,?,'employee',?,?,?,0,?,?,?) ON CONFLICT(company_id,person_id) WHERE person_id IS NOT NULL DO UPDATE SET name=excluded.name").bind(facts.id,facts.companyId,facts.code,facts.name,facts.currency,facts.personId,facts.createdAt,facts.updatedAt),
postPayableInvoice:(facts:{id:string;companyId:string;partyId:string;number:string;invoiceDate:string;dueDate:string;currency:string|number|null;subtotalMinor:string|number|null;taxMinor:string|number|null;totalMinor:number;outstandingMinor:number;debitAccountId:string|number|null;creditAccountId:string|number|null;taxAccountId:string|number|null;expenseId:string;journalId:string;createdBy:string|number|null;approvedBy:string|number|null;postedAt:number;createdAt:number;updatedAt:number})=>db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'payable',?,?,?,?,?,?,?,?,?,?,?,'expense_claim',?,'posted',?,?,?,?,?,?)").bind(facts.id,facts.companyId,facts.partyId,facts.number,facts.invoiceDate,facts.dueDate,facts.currency,facts.subtotalMinor,facts.taxMinor,facts.totalMinor,facts.outstandingMinor,facts.debitAccountId,facts.creditAccountId,facts.taxAccountId,facts.expenseId,facts.journalId,facts.createdBy,facts.approvedBy,facts.postedAt,facts.createdAt,facts.updatedAt),
retainReversalJournal:(facts:{id:string;periodId:string;number:string;date:string;expenseId:string;reason:string;createdBy:string;postedBy:string;postedAt:number;createdAt:number;updatedAt:number;originalJournalId:string|number|null})=>db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'expense_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(facts.id,facts.periodId,facts.number,facts.date,facts.expenseId,facts.reason,facts.createdBy,facts.postedBy,facts.postedAt,facts.createdAt,facts.updatedAt,facts.originalJournalId),
retainReversalLines:(facts:{prefix:string;journalId:string;originalJournalId:string|number|null})=>db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(facts.prefix,facts.journalId,facts.originalJournalId),
markJournalReversed:(facts:{actorId:string;reversedAt:number;updatedAt:number;journalId:string|number|null})=>db.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(facts.actorId,facts.reversedAt,facts.updatedAt,facts.journalId),
voidExpenseInvoice:(facts:{updatedAt:number;invoiceId:string|number|null})=>db.prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?").bind(facts.updatedAt,facts.invoiceId)
};}
