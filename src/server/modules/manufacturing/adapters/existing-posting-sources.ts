import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {manufacturingFinanceRecordSources} from './finance-record-sources';
/** Actual original posting IDs, including an absence witness when a previously
 * recorded posting no longer satisfies its immutable owner/receipt lineage. */
export async function manufacturingExistingPostingEconomicReferences(database:SqlDatabase,companyId:string){
 const rows=(await database.prepare(`SELECT posting.id,posting.order_id,posting.journal_id,posting.amount_minor,to_jsonb(posting)::text image,
 EXISTS(SELECT 1 FROM (${manufacturingFinanceRecordSources.posting}) actual WHERE actual.posting_id=posting.id AND actual.company_id=posting.company_id AND actual.id=posting.journal_id) proven
 FROM manufacturing_order_postings posting WHERE posting.company_id=? ORDER BY posting.id`).bind(companyId).all<{id:string;order_id:string;journal_id:string;amount_minor:number;image:string;proven:boolean}>()).results;
 return {journalIds:[...new Set(rows.map(row=>row.journal_id))],orderIds:[...new Set(rows.map(row=>row.order_id))],invalid:rows.some(row=>Number(row.amount_minor)>0&&!row.proven),image:JSON.stringify(rows)};
}
