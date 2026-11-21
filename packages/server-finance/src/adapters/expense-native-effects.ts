import "server-only";
import { nativeExpenseEffectsSql } from '@erpx/server-collaboration/adapters/native-expense-effects-sql';
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
retainEmployeeParty:(facts:{id:string;companyId:string;code:string;name:string|number|null;currency:string|number|null;personId:string|number|null;createdAt:number;updatedAt:number})=>db.prepare(nativeExpenseEffectsSql('expense_employee_party')).bind(facts.id,facts.companyId,facts.code,facts.name,facts.currency,facts.personId,facts.createdAt,facts.updatedAt),
postPayableInvoice:(facts:{id:string;companyId:string;partyId:string;number:string;invoiceDate:string;dueDate:string;currency:string|number|null;subtotalMinor:string|number|null;taxMinor:string|number|null;totalMinor:number;outstandingMinor:number;debitAccountId:string|number|null;creditAccountId:string|number|null;taxAccountId:string|number|null;expenseId:string;journalId:string;createdBy:string|number|null;approvedBy:string|number|null;postedAt:number;createdAt:number;updatedAt:number})=>db.prepare(nativeExpenseEffectsSql('expense_payable_invoice')).bind(facts.id,facts.companyId,facts.partyId,facts.number,facts.invoiceDate,facts.dueDate,facts.currency,facts.subtotalMinor,facts.taxMinor,facts.totalMinor,facts.outstandingMinor,facts.debitAccountId,facts.creditAccountId,facts.taxAccountId,facts.expenseId,facts.journalId,facts.createdBy,facts.approvedBy,facts.postedAt,facts.createdAt,facts.updatedAt),
retainReversalJournal:(facts:{id:string;periodId:string;number:string;date:string;expenseId:string;reason:string;createdBy:string;postedBy:string;postedAt:number;createdAt:number;updatedAt:number;originalJournalId:string|number|null})=>db.prepare(nativeExpenseEffectsSql('expense_reversal_journal')).bind(facts.id,facts.periodId,facts.number,facts.date,facts.expenseId,facts.reason,facts.createdBy,facts.postedBy,facts.postedAt,facts.createdAt,facts.updatedAt,facts.originalJournalId),
retainReversalLines:(facts:{prefix:string;journalId:string;originalJournalId:string|number|null})=>db.prepare(nativeExpenseEffectsSql('expense_reversal_lines')).bind(facts.prefix,facts.journalId,facts.originalJournalId),
markJournalReversed:(facts:{actorId:string;reversedAt:number;updatedAt:number;journalId:string|number|null})=>db.prepare(nativeExpenseEffectsSql('expense_journal_reversed')).bind(facts.actorId,facts.reversedAt,facts.updatedAt,facts.journalId),
voidExpenseInvoice:(facts:{updatedAt:number;invoiceId:string|number|null})=>db.prepare(nativeExpenseEffectsSql('expense_invoice_voided')).bind(facts.updatedAt,facts.invoiceId)
};}
