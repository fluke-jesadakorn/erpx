import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {CommercePostingSetupConfiguration} from '../../commerce/public/posting-setup';
import {ApiError} from '../../runtime/public/http';
import {validateCommercePostingFinanceReferences} from './commerce-posting-setup';
import {salesGoodsPostingFinanceState} from './sales-goods-posting-setup';
import {manufacturingExistingPostingEconomicReferences} from '../../manufacturing/public/existing-posting-sources';

/** Only the original Manufacturing owner's actual Finance foreign keys. Full
 * owner rows stay private; no caller SQL, source label or Company-wide scan. */
export async function manufacturingExistingPostingFinancialHistory(database:SqlDatabase,actor:WorkContext){
 const source=await manufacturingExistingPostingEconomicReferences(database,actor.companyId);
 const state=await database.prepare(`WITH primary_journals AS MATERIALIZED(SELECT jsonb_array_elements_text(?::jsonb) id),owner_orders AS MATERIALIZED(SELECT jsonb_array_elements_text(?::jsonb) id),books AS MATERIALIZED(SELECT b.* FROM finance_book_postings b JOIN primary_journals p ON p.id=b.primary_journal_id),journals AS MATERIALIZED(SELECT id FROM primary_journals UNION SELECT secondary_journal_id FROM books)
 SELECT jsonb_build_object(
 'journals',(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM finance_journals j JOIN journals actual ON actual.id=j.id),
 'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM finance_journal_lines l JOIN journals actual ON actual.id=l.journal_id),
 'captures',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM finance_financial_events c JOIN journals actual ON actual.id=c.journal_id),
 'books',(SELECT jsonb_agg(to_jsonb(b) ORDER BY b.primary_journal_id,b.secondary_journal_id) FROM books b),
 'money',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.source_id,m.amount_kind) FROM money_transaction_snapshots m WHERE m.company_id=? AND (m.source_id IN(SELECT id FROM journals) OR m.source_id IN(SELECT id FROM owner_orders)))
 )::text image,
 EXISTS(SELECT 1 FROM books b WHERE b.company_id<>? OR NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=b.secondary_journal_id AND j.company_id=b.company_id))
 OR EXISTS(SELECT 1 FROM finance_financial_events c JOIN journals actual ON actual.id=c.journal_id WHERE c.company_id<>?) invalid`).bind(JSON.stringify(source.journalIds),JSON.stringify(source.orderIds),actor.companyId,actor.companyId,actor.companyId).first<{image:string;invalid:boolean}>();
 if(!state)throw new ApiError(503,'MANUFACTURING_POSTING_SOURCE_UNAVAILABLE','The original Manufacturing financial evidence is unavailable.');
 return {image:JSON.stringify([source.image,state.image]),invalid:source.invalid||state.invalid};
}

/** Confirmation retains BOTH the real nine-role row and the original reader's
 * effective reviewed Sales overrides. It never reconciles them by rewriting. */
export async function validateManufacturingExistingPostingFinanceState(database:SqlDatabase,actor:WorkContext,configuration:CommercePostingSetupConfiguration){
 const state=await salesGoodsPostingFinanceState(database,actor),history=await manufacturingExistingPostingFinancialHistory(database,actor);
 await validateCommercePostingFinanceReferences(database,actor,configuration);
 if(state.references.charts.find(row=>row.id===configuration.chartId)?.base_currency!=='THB')throw new ApiError(422,'MANUFACTURING_CURRENCY_ADAPTER','The original production accounting adapter requires a THB functional chart.');
 if(state.historyInvalid||state.defaultsBlocked||history.invalid)throw new ApiError(409,'MANUFACTURING_POSTING_HISTORY_REQUIRES_REVIEW','Reconcile the original financial source before confirming this unchanged mapping.');
 const d=state.defaults;
 if(d){
  if(d.chart_id!==configuration.chartId||d.currency!==configuration.currency)throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','Retain the actual reviewed Sales chart and document currency.');
  await validateCommercePostingFinanceReferences(database,actor,{...configuration,receivableAccountId:d.receivable_account_id,revenueAccountId:d.revenue_account_id,outputTaxAccountId:d.tax_account_id??configuration.outputTaxAccountId});
 }
 return state;
}
