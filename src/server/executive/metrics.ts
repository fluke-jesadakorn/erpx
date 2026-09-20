import {visibility as legalVisibility} from "../legal/access";
import "server-only";
import { hasPermission } from "@/lib/permissions";
import { metricIds, type Breakdown, type Filters, type MetricData, type MetricId, type MetricInfo } from "@/lib/executive/canvas";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { sha256 } from "../crypto";
import { ApiError } from "../http";

const specs:Record<MetricId,{label:string;description:string;unit:"money"|"count";dateBasis:string;permission:string;companyOnly?:boolean}>={
  "legal-records":{label:"Legal records",description:"Records visible to the current reader, grouped by state.",unit:"count",dateBasis:"Last update",permission:"legal.read"},
  "legal-contract-value":{label:"Legal contract value",description:"Contract face values in the selected currency for records visible to the current reader. Not recognized revenue.",unit:"money",dateBasis:"Last update",permission:"legal.read"},
  "legal-exposure":{label:"Legal case exposure",description:"Recorded case exposure in the selected currency for matters visible to the current reader. Not a booked provision.",unit:"money",dateBasis:"Last update",permission:"legal.read"},
  receivables:{label:"Open receivables",description:"Current outstanding amount on posted and partially paid receivable invoices, grouped by status. No draft or void invoices.",unit:"money",dateBasis:"Invoice date; current balance",permission:"finance.read",companyOnly:true},
  payables:{label:"Open payables",description:"Current outstanding amount on posted and partially paid payable invoices, grouped by status.",unit:"money",dateBasis:"Invoice date; current balance",permission:"finance.read",companyOnly:true},
  "invoiced-sales":{label:"Invoiced sales",description:"Posted, partially paid and paid receivable invoice totals including tax, grouped by invoice month. This is invoiced value, not recognized revenue or cash collected.",unit:"money",dateBasis:"Invoice date",permission:"finance.read",companyOnly:true},
  "sales-orders":{label:"Confirmed sales orders",description:"Confirmed, partially fulfilled and fulfilled sales order value including tax, grouped by order month.",unit:"money",dateBasis:"Order date",permission:"commerce.read"},
  "purchase-orders":{label:"Confirmed purchase orders",description:"Confirmed, partially fulfilled and fulfilled purchase order value including tax, grouped by order month.",unit:"money",dateBasis:"Order date",permission:"commerce.read"},
  headcount:{label:"Active headcount",description:"Distinct active people with a currently effective active engagement, counted once across engagements in the selected scope.",unit:"count",dateBasis:"Current effective date; date filter does not apply",permission:"people.read"},
  "my-tasks":{label:"My open tasks",description:"Tasks assigned to the current reader, excluding drafts, completed and cancelled tasks, grouped by state.",unit:"count",dateBasis:"Due date (UTC)",permission:"self.tasks"},
  inventory:{label:"Stocked SKU locations",description:"Product and warehouse pairs with positive on-hand stock, grouped by warehouse. Counts locations, not mixed-unit quantities or inventory value.",unit:"count",dateBasis:"Current stock; date filter does not apply",permission:"commerce.read",companyOnly:true},
  "pos-sales":{label:"Completed POS sales",description:"Completed sale totals including tax and discounts, grouped by business date month. Gross completed sales before subsequent refunds.",unit:"money",dateBasis:"POS business date",permission:"pos.reports"},
  "hotel-rooms":{label:"Configured hotel rooms",description:"Room records grouped by property. This is configured room count, not occupancy or sellable inventory.",unit:"count",dateBasis:"Current configuration; date filter does not apply",permission:"pms.read"},
  projects:{label:"My projects",description:"Projects owned by or explicitly shared with the current reader, grouped by creation month.",unit:"count",dateBasis:"Created date (UTC)",permission:"records.read"},
  routines:{label:"My published routines",description:"Published routine templates assigned to the current reader, grouped by publication status.",unit:"count",dateBasis:"Current template; date filter does not apply",permission:"self.tasks"},
};
export function metricBreakdowns(id:MetricId):Breakdown[] {
  if(id==="headcount")return ["default","branch","department","level"];
  if(["sales-orders","purchase-orders","pos-sales"].includes(id))return ["default","month","status","branch"];
  if(["receivables","payables","invoiced-sales"].includes(id))return ["default","month","status"];
  return ["default"];
}
export function metricCatalog(a:WorkContext):MetricInfo[] {
  return metricIds.map(id=>{const s=specs[id],available=hasPermission(a,s.permission)&&!(s.companyOnly&&(a.branchId||a.departmentId))&&!(a.departmentId&&["pos-sales","hotel-rooms"].includes(id));return {id,label:s.label,description:s.description,unit:s.unit,dateBasis:s.dateBasis,breakdowns:metricBreakdowns(id),available:!!available,...(!available?{reason:"The selected work role or scope does not allow this metric."}:{})};});
}
/** Only explicit, fixed query adapters enter this gateway; no model-authored SQL or URL. */
export async function queryMetric(db:SqlDatabase,a:WorkContext,id:MetricId,f:Filters,breakdown:Breakdown="default"):Promise<MetricData> {
  const spec=specs[id];
  if(!metricBreakdowns(id).includes(breakdown))throw new ApiError(422,"BREAKDOWN_UNAVAILABLE","This metric does not support that grouping.");
  if(!metricCatalog(a).find(m=>m.id===id)?.available)throw new ApiError(403,"METRIC_FORBIDDEN","This metric is unavailable in the current source permissions or scope.");
  const scope=(alias:string)=>`${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`;
  const args=[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
  let sql="",values:unknown[]=[];
  if(["receivables","payables","invoiced-sales"].includes(id)) {
    const sales=id==="invoiced-sales";
    sql=`SELECT ${breakdown==="month"||breakdown==="default"&&sales?"substring(invoice_date,1,7)":"status"} label,sum(${sales?"total_minor":"outstanding_minor"}) value FROM finance_invoices WHERE company_id=? AND kind=? AND currency=? AND invoice_date BETWEEN ? AND ? AND status IN (${sales?"'posted','partially_paid','paid'":"'posted','partially_paid'"}) GROUP BY 1 ORDER BY 1`;
    values=[a.companyId,id==="payables"?"payable":"receivable",f.currency,f.from,f.to];
  } else if(id==="sales-orders"||id==="purchase-orders") {
    sql=`SELECT ${breakdown==="status"?"o.state":breakdown==="branch"?"COALESCE((SELECT display_name FROM organization_nodes WHERE id=o.branch_id AND company_id=o.company_id),'Company-wide')":"substring(o.order_date,1,7)"} label,sum(o.subtotal_minor+o.tax_minor) value FROM commerce_orders o WHERE ${scope("o")} AND o.kind=? AND o.currency=? AND o.order_date BETWEEN ? AND ? AND o.state IN ('confirmed','partially_fulfilled','fulfilled') GROUP BY 1 ORDER BY 1`;
    values=[...args,id==="sales-orders"?"sales":"purchase",f.currency,f.from,f.to];
  } else if(id==="headcount") {
    // One effective engagement per person keeps group totals additive, with deterministic tie-breaking.
    const label=breakdown==="branch"||breakdown==="department"?`COALESCE((SELECT display_name FROM organization_nodes WHERE id=e.${breakdown}_id AND company_id=e.company_id),'Unassigned')`:breakdown==="level"?"COALESCE((SELECT 'Level '||level_number||' · '||name FROM staff_levels WHERE id=e.level_id AND company_id=e.company_id),'Unmapped')":"'Active people'";
    sql=`WITH effective AS (SELECT DISTINCT ON (p.id) p.id person_id,e.* FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE ${scope("e")} AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY p.id,e.start_date DESC,e.id) SELECT ${label} label,count(*) value FROM effective e GROUP BY 1 ORDER BY 1`;
    values=args;
  } else if(id==="my-tasks") {
    sql=`SELECT t.state label,count(*) value FROM staff_tasks t WHERE ${scope("t")} AND t.assignee_user_id=? AND t.state NOT IN ('draft','completed','cancelled') AND t.due_at>=? AND t.due_at<? GROUP BY 1 ORDER BY 1`;
    values=[...args,a.userId,Date.parse(f.from),Date.parse(f.to)+86400000];
  } else if(id==="inventory") {
    sql="SELECT w.name label,count(*) value FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND s.quantity_milli>0 GROUP BY w.id,w.name ORDER BY 1";values=[a.companyId];
  } else if(id==="pos-sales") {
    sql=`SELECT ${breakdown==="status"?"s.state":breakdown==="branch"?"COALESCE((SELECT display_name FROM organization_nodes WHERE id=s.branch_id AND company_id=s.company_id),'Company-wide')":"substring(s.business_date,1,7)"} label,sum(s.total_minor) value FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=? AND s.business_date BETWEEN ? AND ? AND s.state='completed' GROUP BY 1 ORDER BY 1`;values=[a.companyId,a.branchId,a.branchId,f.currency,f.from,f.to];
  } else if(id==="hotel-rooms") {
    sql="SELECT p.name label,count(*) value FROM pms_records r JOIN pms_properties p ON p.id=r.property_id WHERE p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND r.kind='room' GROUP BY p.id,p.name ORDER BY 1";values=[a.companyId,a.branchId,a.branchId];
  } else if(id==="projects") {
    sql=`SELECT to_char(to_timestamp(p.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*) value FROM staff_projects p WHERE ${scope("p")} AND (p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)) AND p.created_at>=? AND p.created_at<? GROUP BY 1 ORDER BY 1`;values=[...args,a.userId,a.userId,Date.parse(f.from),Date.parse(f.to)+86400000];
  } else {
    sql=`SELECT 'Published' label,count(*) value FROM routine_templates t WHERE ${scope("t")} AND t.assignee_user_id=? AND t.published=1`;values=[...args,a.userId];
  }
  if(id.startsWith("legal-")) {const v=legalVisibility(a);const table=id==="legal-contract-value"?"legal_contracts":id==="legal-exposure"?"legal_matters":null;const field=id==="legal-contract-value"?"value":"exposure";
    sql=`SELECT i.state label,${table?`coalesce(sum(coalesce((t.data->>'${field}')::bigint,0)),0)`:"count(*)"} value FROM legal_items i ${table?`JOIN ${table} t ON t.item_id=i.id`:""} WHERE ${v.sql} AND i.updated_at>=? AND i.updated_at<? ${table?"AND i.currency=?":""} GROUP BY i.state ORDER BY i.state`;
    values=[...v.values,Date.parse(f.from),Date.parse(f.to)+86400000,...(table?[f.currency]:[])];
  }
  const raw=(await db.prepare(sql).bind(...values).all<{label:string;value:number}>()).results;
  const all=raw.map(r=>({...r,value:Number(r.value)}));
  const total=all.reduce((s,r)=>s+Number(r.value),0);
  if(!Number.isSafeInteger(total))throw new ApiError(422,"METRIC_OVERFLOW","This total exceeds the supported exact range.");
  // Exact total precedes bounded display rows. Never derive a total from a capped list.
  const rows=all.length>100?[...all.slice(0,99),{label:"Other",value:all.slice(99).reduce((s,r)=>s+Number(r.value),0)}]:all;
  return {metric:id,total,rows,unit:spec.unit,currency:spec.unit==="money"?f.currency:null,definition:`${spec.description} Date basis: ${spec.dateBasis}. Grouping: ${breakdown}. ${id==="headcount"&&breakdown!=="default"?"For people with multiple effective engagements, the latest start date then engagement ID chooses one group.":""}`,scope:`${a.departmentId?"Selected department":a.branchId?"Selected branch":"Selected company"}${["my-tasks","projects","routines"].includes(id)?" · Current reader only":""}`,updatedAt:Date.now(),revision:await sha256(JSON.stringify({id,breakdown,all,f,scope:[a.companyId,a.branchId,a.departmentId,a.userId]}))};
}
