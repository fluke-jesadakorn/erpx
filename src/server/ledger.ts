import { readListPage } from "./list-page";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { ApiError } from "./http";
import { listCharts,statements } from "./finance";
export type LedgerRow=Record<string,string|number|null>;
export async function ledgerWorkspace(actor:WorkContext,chartId?:string,query?:{status?:string;cursor?:string;print?:string}) {
  const charts=await listCharts(actor);const chart=charts.find(c=>c.id===chartId)??charts.find(c=>c.name==="Operations ledger")??charts[0];if(!chart)throw new ApiError(409,"CHART_REQUIRED","Create a chart of accounts first.");
  const {write}=await holdingResources(actor.holdingId);
  const [accounts,periods,journals,report]=await Promise.all([
    write.prepare("SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? ORDER BY code").bind(chart.id,actor.companyId).all<LedgerRow>(),
    write.prepare("SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC").bind(actor.companyId).all<LedgerRow>(),
    query && query.print !== '1' ? readListPage<LedgerRow & {id:string;page_key:string}>(write,{
      sql:`SELECT j.*,j.created_at::text page_key,COALESCE((SELECT SUM(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0) debit,
        COALESCE((SELECT SUM(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0) credit FROM finance_journals j
        WHERE j.chart_id=? AND j.company_id=? AND (? IN ('','all') OR j.status=?)`,
      args:[chart.id,actor.companyId,query.status??'',query.status??''],cursor:query.cursor,descending:true,scope:[actor.companyId,chart.id,query.status??''],
    }).then(page=>({results:page.items,page})) : write.prepare("SELECT j.*,COALESCE(SUM(l.debit_minor),0) debit,COALESCE(SUM(l.credit_minor),0) credit FROM finance_journals j LEFT JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.chart_id=? AND j.company_id=? GROUP BY j.id ORDER BY j.created_at DESC,j.id").bind(chart.id,actor.companyId).all<LedgerRow>().then(result=>({...result,page:null})),
    statements(actor,undefined,String(chart.id)),
  ]);return {charts:charts as LedgerRow[],chart:chart as LedgerRow,accounts:accounts.results,periods:periods.results,journals:journals.results,journalPage:journals.page,report};
}
export async function workJournalRows(actor: WorkContext) {
  await listCharts(actor);
  const { write } = await holdingResources(actor.holdingId);
  return (await write.prepare("SELECT * FROM finance_journals WHERE company_id=? ORDER BY created_at DESC").bind(actor.companyId).all<LedgerRow>()).results;
}
export async function journalDetail(actor:WorkContext,id:string) {
  await listCharts(actor);const {write}=await holdingResources(actor.holdingId);const journal=await write.prepare("SELECT j.*,COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.created_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account') created_by_name,COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.posted_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account') posted_by_name FROM finance_journals j WHERE j.id=? AND j.company_id=?").bind(id,actor.companyId).first<LedgerRow>();if(!journal)throw new ApiError(404,"JOURNAL_NOT_FOUND","Journal not found.");
  const lines=await write.prepare("SELECT l.*,a.code,a.name FROM finance_journal_lines l JOIN finance_accounts a ON a.id=l.account_id WHERE l.journal_id=? ORDER BY l.ordinal").bind(id).all<LedgerRow>();return {journal,lines:lines.results};
}
