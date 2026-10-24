import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {payrollOtherRemittanceTables,type PayrollOtherSource} from '@erpx/server-people_payroll/public/payroll-other-remittance-read-sources';
export const payrollOtherBankMarker='__payrollOtherLiabilityBankEvidence';
export type NativeRow=Record<string,unknown>;
export function payrollOtherFinanceLeases(database:SqlDatabase,write:boolean){return [database.prepare('LOCK TABLE finance_charts IN SHARE MODE'),database.prepare(`LOCK TABLE finance_accounts,finance_bank_statements,finance_bank_transactions,finance_journals,finance_journal_lines,finance_financial_events,finance_book_postings,money_transaction_snapshots IN ${write?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`),database.prepare('LOCK TABLE finance_periods,finance_book_rules,finance_book_account_mappings,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE')];}
const rows=async(database:SqlDatabase,sql:string,args:string[]) =>(await database.prepare(sql).bind(...args).all<{image:string}>()).results.map(row=>JSON.parse(row.image) as NativeRow);
export async function payrollOtherFinanceState(database:SqlDatabase,companyId:string,source:PayrollOtherSource){
 const journal=await database.prepare("SELECT to_jsonb(j)::text image FROM finance_journals j WHERE j.id=? AND j.company_id=? AND j.status='posted' AND j.source_type='payroll' AND j.source_id=? AND j.currency=?").bind(source.journalId,companyId,source.run.id,source.run.currency).first<string>('image');
 if(!journal)throw new ApiError(409,'PAYROLL_POSTING_MISSING','Use the original posted Payroll accrual journal.');const header=JSON.parse(journal) as NativeRow;
 const [lines,charts,accounts,periods,money,scales,rates,rules,mappings,remittances,evidence]=await Promise.all([
  rows(database,'SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE l.journal_id=? ORDER BY l.ordinal,l.id',[source.journalId]),
  rows(database,'SELECT to_jsonb(c)::text image FROM finance_charts c WHERE c.id=? AND c.company_id=?',[String(header.chart_id),companyId]),
  rows(database,'SELECT to_jsonb(a)::text image FROM finance_accounts a WHERE a.chart_id=? AND a.company_id=? ORDER BY a.id',[String(header.chart_id),companyId]),
  rows(database,'SELECT to_jsonb(p)::text image FROM finance_periods p WHERE p.company_id=? ORDER BY p.id',[companyId]),
  rows(database,'SELECT to_jsonb(m)::text image FROM company_money_settings m WHERE m.company_id=?',[companyId]),
  rows(database,'SELECT to_jsonb(m)::text image FROM company_currency_scales m WHERE m.company_id=? ORDER BY m.currency',[companyId]),
  rows(database,'SELECT to_jsonb(r)::text image FROM company_fx_rates r WHERE r.company_id=? ORDER BY r.id',[companyId]),
  rows(database,'SELECT to_jsonb(r)::text image FROM finance_book_rules r WHERE r.company_id=? ORDER BY r.id',[companyId]),
  rows(database,'SELECT to_jsonb(m)::text image FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.company_id=? ORDER BY m.rule_id,m.primary_account_id',[companyId]),
  rows(database,`SELECT to_jsonb(r)::text image FROM (${payrollOtherRemittanceTables.remittances}) r WHERE r.company_id=? AND r.run_id=? ORDER BY r.id`,[companyId,source.run.id]),
  database.prepare(`SELECT c.command_id,c.actor_id,c.request_hash,c.result_json,c.authority_json,to_jsonb(s)::text statement,to_jsonb(t)::text movement,(SELECT jsonb_agg(to_jsonb(ignored) ORDER BY ignored.command_id)::text FROM command_receipts ignored WHERE ignored.operation='payroll.liability.bank-evidence.ignore' AND ignored.record_id=t.id AND ignored.authority_json::jsonb->>'companyId'=s.company_id) ignored_receipts FROM command_receipts c JOIN finance_bank_statements s ON s.id=c.record_id AND s.company_id=? JOIN finance_bank_transactions t ON t.statement_id=s.id AND t.company_id=s.company_id WHERE c.operation='finance.bank.import' AND c.authority_json::jsonb->>'companyId'=s.company_id AND c.result_json::jsonb->'${payrollOtherBankMarker}'->>'runId'=? ORDER BY c.command_id,t.id`).bind(companyId,source.run.id).all<{command_id:string;actor_id:string;request_hash:string;result_json:string;authority_json:string;statement:string;movement:string;ignored_receipts:string|null}>(),
 ]);
 const references={charts,accounts,periods,money,scales,rates,rules,mappings};
 return {header,lines,accrualImage:JSON.stringify([header,lines]),references,referenceImage:JSON.stringify(references),remittances,evidence:evidence.results};
}
export type PayrollOtherFinanceState=Awaited<ReturnType<typeof payrollOtherFinanceState>>;
export function payrollOtherReferencesUnchanged(before:PayrollOtherFinanceState,after:PayrollOtherFinanceState,bankPromotion?:string){
 const original=structuredClone(before.references),current=structuredClone(after.references);if(bankPromotion){const left=original.accounts.find(row=>row.id===bankPromotion),right=current.accounts.find(row=>row.id===bankPromotion);if(left&&right&&left.control_kind==='none'&&right.control_kind==='bank'&&Number(right.version)===Number(left.version)+1&&Number(right.is_control)===1){for(const field of ['control_kind','is_control','version','updated_at']){delete left[field];delete right[field];}}}return JSON.stringify(original)===JSON.stringify(current);
}
export async function payrollOtherJournalImage(database:SqlDatabase,companyId:string,id:string){return {journal:await database.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE j.id=? AND j.company_id=?').bind(id,companyId).first<string>('image'),lines:(await database.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE l.journal_id=? ORDER BY l.ordinal,l.id').bind(id).all<{image:string}>()).results};}
