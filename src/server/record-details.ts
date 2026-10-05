import { fiscalCloseSourceSql } from "./finance-fiscal-close-source";
import {agentEvidenceRecord} from "./agent-governance";
import {isFinanceExpansionKind,financeExpansionRecord} from './finance-expansion-records';
import {groupExpansionRecord,groupEditionSourceJournal,groupEditionAccountSources} from './finance-group-editions';
import {financeCompanyTools} from './finance-company-tools';
import {financeExtraRecord,isFinanceExtraKind} from './finance-record-details';
import {pmsRefundRecord,pmsSnapshotEntry,pmsControlSources} from './pms/record-details';
import {queryMetric} from './executive/metrics';
import {metricIds,breakdowns,filtersSchema} from '@/lib/executive/canvas';
import "server-only";
import { isDomainRecordKind, recordDomainData } from "./record-domain-details";
import { z } from "zod";
import { commerceOverview, commercePermissions, type Row } from "./commerce";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { ApiError } from "./http";
import { hasPermission } from "@/lib/permissions";
import { bankWorkspace, bankStatementDetail, listCharts, listAccounts, aging, reportDateSchema, trialBalance, statements } from "./finance";
import { marketingWorkspace } from "./marketing";
import { payrollSummary } from "./people-payroll";
import { calculationRunDetail } from "./finance-engines";
import { accrualRecordDetail } from "./finance-accruals";
import { intercompanyDetail } from "./finance-intercompany";
import { consolidationDetail,consolidationEliminationDetail } from "./finance-consolidation";
import { pricingWorkspace } from "./sales-pricing";
import { listInventoryReorderPolicies } from "./inventory-reorder-policies";
import type { RecordTarget } from "@/lib/record-drawer";
import { posReportReceipts } from "./pos";
import { statementSourceAccounts } from "./finance-reporting";
import {financePlanRecord,budgetSourceRecords,financialReportSourceRecords} from './finance-planning';
import {financeBookRecord} from './finance-books';

export async function commerceMasterDetail(actor: WorkContext, kind: "product" | "party" | "warehouse", id: string) {
  requirePermission(actor, "commerce.read");
  const { write } = await holdingResources(actor.holdingId);
  const row = kind === "product"
    ? await write.prepare("SELECT * FROM commerce_products WHERE id=? AND company_id=?").bind(id,actor.companyId).first<Row>()
    : kind === "party"
    ? await write.prepare("SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind IN ('customer','supplier') AND person_id IS NULL").bind(id,actor.companyId).first<Row>()
    : await write.prepare("SELECT w.*,b.display_name branch_name FROM commerce_warehouses w JOIN organization_nodes b ON b.id=w.branch_id WHERE w.id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)").bind(id,actor.companyId,actor.branchId,actor.branchId).first<Row>();
  if (!row) throw new ApiError(404,"RECORD_NOT_FOUND","Record not found in this workspace.");
  const permissions = commercePermissions(actor), canEdit = kind === "party" ? row.kind === "supplier" ? permissions.purchase : permissions.sales : permissions.stock;
  const options = canEdit ? await commerceOverview(actor) : null;
  const history = (await write.prepare("SELECT action,actor_id,created_at,record_version FROM audit_events WHERE record_id=? ORDER BY created_at DESC LIMIT 50").bind(id).all<Row>()).results;
  return { kind, row, options, history, canEdit };
}
export function drilldownPage(value?: string) { return z.coerce.number().int().min(1).max(100000).default(1).parse(value); }
export async function recordSupplementData(actor: WorkContext, target: RecordTarget, args: Record<string,string>) {
  const { write } = await holdingResources(actor.holdingId), page = drilldownPage(args.detailPage), offset = (page-1)*30;
  if(target.kind==='executive_sources'||target.kind==='executive_source'){
    const metric=z.enum(metricIds).parse(target.kind==='executive_sources'?target.id:args.metric),breakdown=z.enum(breakdowns).default('default').parse(args.breakdown),filters=filtersSchema.parse({from:args.from,to:args.to,currency:args.currency});
    const result=await queryMetric(write,actor,metric,filters,breakdown,{label:z.string().min(1).max(2000).parse(args.label),unit:args.unit,page,recordId:target.kind==='executive_source'?target.id:undefined});
    if(target.kind==='executive_source'){
      const source=result.rows[0];if(!source)throw new ApiError(404,'SOURCE_NOT_FOUND','Source record not found in this report group.');
      return {kind:target.kind,row:{...source.facts,id:source.id,name:source.facts.name??source.facts.title??source.facts.number??source.id},related:source.owner_kind&&source.owner_id?[{kind:source.owner_kind,id:source.owner_id,args:source.owner_args??undefined}]:[]};
    }
    return {kind:target.kind,...result};
  }
  if(isFinanceExtraKind(target.kind))return financeExtraRecord(actor,target.kind,target.id,args,page);
  if(isDomainRecordKind(target.kind))return await recordDomainData(actor,target.kind,target.id);
  if(target.kind==='pos_report')return {kind:target.kind,...await posReportReceipts(actor,{businessDate:z.iso.date().parse(target.id),registerId:z.string().min(1).max(180).parse(args.registerId),shiftId:z.string().min(1).max(180).parse(args.shiftId),currency:z.string().length(3).parse(args.currency)},page)};
  if(target.kind==='statement_sources')return {kind:target.kind,...await statementSourceAccounts(actor,target.id,args,page)};
  if(target.kind==='pms_booking_refund'||target.kind==='pms_counter_refund')return {kind:target.kind,...await pmsRefundRecord(actor,target.kind==='pms_booking_refund'?'booking':'counter',target.id)};
  if(target.kind==='pms_snapshot_entry')return {kind:target.kind,...await pmsSnapshotEntry(actor,target.id,z.string().min(1).max(180).parse(args.documentId))};
  if(target.kind==='pms_control_sources')return {kind:target.kind,...await pmsControlSources(actor,target.id,args.control,page)};
  if(target.kind==='consolidation_elimination')return {kind:target.kind,...await consolidationEliminationDetail(actor,target.id)};
  if(target.kind==='agent_invocation'||target.kind==='agent_session')return {kind:target.kind,...await agentEvidenceRecord(actor,target.kind,target.id)};
  if(isFinanceExpansionKind(target.kind))return financeExpansionRecord(actor,target.kind,target.id);
  if(target.kind==='finance_group_edition'||target.kind==='finance_group_ownership'||target.kind==='finance_group_acquisition'||target.kind==='finance_group_pair')return {kind:target.kind,...await groupExpansionRecord(actor,target.kind,target.id),enabled:(await financeCompanyTools(actor)).some(tool=>tool.tool==='group_reporting'&&tool.enabled===1)};
  if(target.kind==='finance_group_sources')return {kind:target.kind,...await groupEditionAccountSources(actor,target.id,z.string().min(1).max(200).parse(args.accountCode),page)};
  if(target.kind==='finance_group_source'){
    const source=await groupEditionSourceJournal(actor,z.string().min(1).max(180).parse(args.editionId),target.id);
    return {kind:target.kind,row:{...source.lines[0],id:target.id,name:`${source.entity.reporting_name} · ${source.lines[0]?.entry_date} · Retained journal`},retainedLines:source.lines};
  }
  if(target.kind==='finance_plan')return {kind:target.kind,...await financePlanRecord(actor,target.id)};
  if(target.kind==='finance_book')return {kind:target.kind,...await financeBookRecord(actor,target.id)};
  if(target.kind==='finance_budget_sources')return {kind:target.kind,...await budgetSourceRecords(actor,target.id,z.coerce.number().int().min(0).max(9999).parse(args.line),page)};
  if(target.kind==='finance_report_snapshot')return {kind:target.kind,...await financialReportSourceRecords(actor,target.id,{},page)};
  if(target.kind==='finance_report_row')return {kind:target.kind,...await financialReportSourceRecords(actor,target.id,args,page)};
  if(target.kind==='inventory_movement'){
    requirePermission(actor,'commerce.read');requirePermission(actor,'inventory.read');
    const row=await write.prepare(`SELECT m.*,p.sku,p.name,p.unit,w.name warehouse_name,c.base_currency currency FROM inventory_ledger m JOIN commerce_products p ON p.id=m.product_id JOIN commerce_warehouses w ON w.id=m.warehouse_id LEFT JOIN commerce_settings s ON s.company_id=w.company_id LEFT JOIN finance_charts c ON c.id=s.chart_id WHERE m.id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`).bind(target.id,actor.companyId,actor.branchId,actor.branchId).first<Row>();
    if(!row)throw new ApiError(404,'MOVEMENT_NOT_FOUND','Inventory movement not found in this workspace.');
    const related:RecordTarget[]=[{kind:'product',id:String(row.product_id)},{kind:'stock_position',id:String(row.product_id),args:{warehouseId:String(row.warehouse_id)}}];
    const sourceKinds:Record<string,RecordTarget['kind']>={fulfillment:'order',customer_return:'order',supplier_return:'order',stock_count:'stock_count',transfer:'stock_transfer',maintenance:'maintenance',field_service:'field_service',manufacturing:'manufacturing_order',landed_cost:'landed_cost'};
    const sourceType=String(row.source_type),kind=sourceKinds[sourceType]??(sourceType.startsWith('transit')?'stock_transit':sourceType.startsWith('manufacturing')?'manufacturing_order':null);
    if(kind)related.push({kind,id:String(row.source_id)});
    if(sourceType.startsWith('pos_')&&hasPermission(actor,'pos.read')&&!actor.departmentId){
      const sales=(await write.prepare(`SELECT DISTINCT s.id FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (s.id=? OR EXISTS(SELECT 1 FROM pos_lines l WHERE l.sale_id=s.id AND l.fulfillment_id=?) OR EXISTS(SELECT 1 FROM pos_refunds r WHERE r.sale_id=s.id AND r.id=?))`).bind(actor.companyId,actor.branchId,actor.branchId,row.source_id,row.source_id,row.source_id).all<Row>()).results;
      related.push(...sales.map(sale=>({kind:'pos_sale' as const,id:String(sale.id)})));
    }
    return {kind:target.kind,row,related};
  }
  if (target.kind === "product" || target.kind === "party" || target.kind === "warehouse") return commerceMasterDetail(actor,target.kind,target.id);
  if (target.kind === "stock_position" || target.kind === "inventory_lot") {
    requirePermission(actor,"commerce.read"); requirePermission(actor,"inventory.read");
    if (target.kind === "inventory_lot") {
      const row = await write.prepare("SELECT l.*,p.sku,p.name,p.unit FROM inventory_lots l JOIN commerce_products p ON p.id=l.product_id WHERE l.id=? AND l.company_id=? AND EXISTS(SELECT 1 FROM inventory_lot_balances b JOIN commerce_warehouses w ON w.id=b.warehouse_id WHERE b.lot_id=l.id AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?))").bind(target.id,actor.companyId,actor.companyId,actor.branchId,actor.branchId).first<Row>();
      if(!row)throw new ApiError(404,"RECORD_NOT_FOUND","Lot not found in this workspace.");
      const rows=(await write.prepare("SELECT b.*,w.name warehouse_name FROM inventory_lot_balances b JOIN commerce_warehouses w ON w.id=b.warehouse_id WHERE b.lot_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?) ORDER BY w.name LIMIT 31 OFFSET ?").bind(target.id,actor.companyId,actor.branchId,actor.branchId,offset).all<Row>()).results;
      return {kind:target.kind,row,rows,page,hasNext:rows.length>30};
    }
    const warehouseId=z.string().min(1).max(180).parse(args.warehouseId);
    const row=await write.prepare(`SELECT s.*,p.sku,p.name,p.unit,w.name warehouse_name,
      COALESCE((SELECT SUM(r.quantity_milli) FROM inventory_active_reservations r WHERE r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id),0) reserved_milli,
      inventory_quarantined_quantity(s.warehouse_id,s.product_id) quarantined_milli
      FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id JOIN commerce_warehouses w ON w.id=s.warehouse_id
      WHERE s.product_id=? AND s.warehouse_id=? AND w.company_id=? AND (?::text IS NULL OR w.branch_id=?)`).bind(target.id,warehouseId,actor.companyId,actor.branchId,actor.branchId).first<Row>();
    if(!row)throw new ApiError(404,"RECORD_NOT_FOUND","Stock position not found in this workspace.");
    const rows=(await write.prepare("SELECT * FROM inventory_ledger WHERE warehouse_id=? AND product_id=? ORDER BY movement_date DESC,id DESC LIMIT 31 OFFSET ?").bind(warehouseId,target.id,offset).all<Row>()).results;
    return {kind:target.kind,row,rows,page,hasNext:rows.length>30};
  }
  if(target.kind==='bank_transaction') {
    const data=await bankWorkspace(actor,{match:target.id});
    return {kind:target.kind,row:data.selectedTransaction!,data};
  }
  if(target.kind==='bank_statement')return {kind:target.kind,...await bankStatementDetail(actor,target.id,page)};
  if(target.kind==='marketing_campaign_sources') {
    const data=await marketingWorkspace(actor),source=z.string().max(180).parse(args.source),all=data.candidates.filter(row=>{
      const attribution=JSON.parse(String(row.attribution_json??'{}'));
      const normalized=(value:unknown)=>typeof value==='string'&&value.trim()?value.trim():'Unspecified';
      return normalized(attribution.campaign)===target.id;
    });
    const matching=all.filter(row=>{const a=JSON.parse(String(row.attribution_json??'{}'));return (String(a.source??'').trim()||String(a.utmSource??'').trim()||'Unspecified')===source;});
    return {kind:target.kind,row:{name:target.id,source,count:matching.length},rows:matching.slice(offset,offset+30),page,hasNext:matching.length>offset+30};
  }
  if (target.kind === "finance_report") {
    const name=z.enum(['assets','liabilities','equity','revenue','expenses']).parse(target.id),chartId=z.string().min(1).parse(args.chartId),dates=reportDateSchema.parse({startDate:args.startDate||undefined,endDate:args.endDate||undefined});
    const report=await statements(actor,undefined,chartId,dates),income=['revenue','expenses'].includes(name),sourceArgs={chartId,...(income&&dates.startDate?{startDate:dates.startDate}:{}),...(dates.endDate?{endDate:dates.endDate}:{}),...(income?{excludeYearClose:'1'}:{})};
    const categories=name==='assets'?['asset']:name==='liabilities'?['liability']:name==='equity'?['equity','revenue','other_income','expense','other_expense']:name==='revenue'?['revenue']:['expense'];
    const all=(await trialBalance(actor,undefined,chartId,income?dates:{endDate:dates.endDate},{excludeYearClose:income})).filter(row=>categories.includes(String(row.category)));
    const total=name==='assets'?report.balanceSheet.assets:name==='liabilities'?report.balanceSheet.liabilities:name==='equity'?report.balanceSheet.equity:name==='revenue'?report.profitAndLoss.revenue:report.profitAndLoss.expenses;
    return {kind:target.kind,row:{name,reported_total:total},rows:all.slice(offset,offset+30),sourceArgs,page,hasNext:all.length>offset+30};
  }
  if (target.kind === "finance_account" || target.kind === "trial_balance") {
    if(args.statementId)await statementSourceAccounts(actor,args.statementId,{period:'current'},1);
    const charts=await listCharts(actor);
    let account=await write.prepare("SELECT a.*,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=?").bind(target.id,actor.companyId).first<Row>();
    if(!account||!charts.some(chart=>chart.id===account!.chart_id))throw new ApiError(404,"ACCOUNT_NOT_FOUND","Account not found.");
    // Apply the same report validation and posted/reversed predicates as the owner.
    const dates=reportDateSchema.parse({startDate:args.startDate||undefined,endDate:args.endDate||undefined});
    if(args.chartId&&args.chartId!==account.chart_id)throw new ApiError(404,"ACCOUNT_NOT_FOUND","Account is outside the selected ledger.");
    await listAccounts(actor,String(account.chart_id));
    if(args.periodId){const period=await write.prepare("SELECT start_date,end_date FROM finance_periods WHERE id=? AND company_id=?").bind(args.periodId,actor.companyId).first<Row>();if(!period)throw new ApiError(404,"PERIOD_NOT_FOUND","Period not found.");dates.startDate=String(period.start_date);dates.endDate=String(period.end_date);}
    const totals=await trialBalance(actor,undefined,String(account.chart_id),dates,{excludeYearClose:args.excludeYearClose==='1'});
    account={...account,...totals.find(row=>row.id===target.id)};
    const rows=(await write.prepare(`SELECT l.*,j.id journal_id,j.journal_number,j.entry_date,j.currency,j.status FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND (?='0' OR NOT ${fiscalCloseSourceSql()}) AND (?::text IS NULL OR j.entry_date>=?) AND (?::text IS NULL OR j.entry_date<=?) ORDER BY j.entry_date DESC,j.id,l.ordinal LIMIT 31 OFFSET ?`).bind(target.id,actor.companyId,account.chart_id,args.excludeYearClose==='1'?'1':'0',dates.startDate??null,dates.startDate??null,dates.endDate??null,dates.endDate??null,offset).all<Row>()).results;
    return {kind:target.kind,row:account,rows,page,hasNext:rows.length>30};
  }
  if (target.kind === "aging_bucket") {
    const kind=z.enum(['payable','receivable']).parse(args.kind),asOf=z.iso.date().parse(args.asOf),currency=z.string().length(3).parse(args.currency),bucket=z.enum(['current','1-30','31-60','61-90','90+']).parse(target.id);
    const all=(await aging(actor,kind,asOf,args.chartId)).filter(row=>row.bucket===bucket&&(row as unknown as Row).currency===currency);
    return {kind:target.kind,row:{name:`${kind} · ${bucket} · ${currency}`,asOf,currency,count:all.length,outstanding:all.reduce((sum,row)=>sum+Number(row.outstanding_minor),0)},rows:all.slice(offset,offset+30),page,hasNext:all.length>offset+30};
  }
  if (target.kind === "payroll_month") {
    const currency=z.string().length(3).parse(args.currency),summary=await payrollSummary(actor,target.id);
    const row=summary.find(row=>row.currency===currency);
    if(!row)throw new ApiError(404,"PAYROLL_MONTH_NOT_FOUND","No posted payroll in this month and currency.");
    const rows=(await write.prepare(`SELECT r.id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,COUNT(i.id) headcount,SUM(i.gross_minor) gross_minor,SUM(i.deductions_minor) deductions_minor,SUM(i.net_minor) net_minor FROM hr_payroll_runs r JOIN hr_payroll_items i ON i.run_id=r.id WHERE r.company_id=? AND r.status='posted' AND substring(r.pay_date FROM 1 FOR 7)=? AND r.currency=? GROUP BY r.id ORDER BY r.pay_date DESC,r.id LIMIT 31 OFFSET ?`).bind(actor.companyId,target.id,currency,offset).all<Row>()).results;
    return {kind:target.kind,row:{...row,name:`Posted payroll · ${target.id}`},rows,page,hasNext:rows.length>30};
  }
  if (target.kind === "accrual_schedule" || target.kind === "accrual_run") return {kind:target.kind,...await accrualRecordDetail(actor,target.kind === "accrual_schedule" ? "schedule" : "run",target.id)};
  if (target.kind === "intercompany") return {kind:target.kind,...await intercompanyDetail(actor,target.id)};
  if (target.kind === "consolidation") return {kind:target.kind,...await consolidationDetail(actor,target.id)};
  if (target.kind === "price_list") {
    const data=await pricingWorkspace(actor),row=data.lists.find(row=>row.id===target.id);
    if(!row)throw new ApiError(404,"PRICE_LIST_NOT_FOUND","Price list not found.");
    return {kind:target.kind,row,data:{...data,lists:[row],items:data.items.filter(item=>item.price_list_id===target.id)}};
  }
  if (target.kind === "reorder_policy") {
    const row=(await listInventoryReorderPolicies(actor)).find(row=>row.id===target.id);
    if(!row)throw new ApiError(404,"REORDER_POLICY_NOT_FOUND","Reorder policy not found.");
    const options=await commerceOverview(actor);
    return {kind:target.kind,row,options};
  }
  if (target.kind === "calculation_run") { const run=await calculationRunDetail(actor,target.id); return {kind:target.kind,row:run,rows:run.events,page:1,hasNext:false}; }
  throw new ApiError(404,"RECORD_KIND_UNAVAILABLE","This record does not have a detail reader.");
}
