import type {WorkContext} from "../../../identity_access/public/authorization";
import type {MetricData,MetricId,Filters,Breakdown,defaultFilters} from "@/server/modules/runtime/public/rules/canvas-domain";
import type {reportAccess,reportData} from "../../public/executive/reports";
import type {metricCatalogFor} from "../../public/executive/metrics";
import type {Clock} from "../../../shared/application/ports";
import {executiveToolSchemas,metricProjection,type ToolName} from "../../domain/executive/mcp";
const TOOL_RESULT_BYTES=128*1024;
const widgetProjection=(w:import("@/server/modules/runtime/public/rules/canvas-domain").Widget)=>({id:w.id,title:w.title,description:w.description,metric:w.metric,kind:w.kind,breakdown:w.breakdown??"default",comparePeriod:w.comparePeriod??null});
export type ExecutiveMcpUnit={dashboards(actor:WorkContext,cursor:string,now:number,limit:number):Promise<{results:{id:string}[]}>;access(actor:WorkContext,id:string):ReturnType<typeof reportAccess>;data(actor:WorkContext,id:string,filters?:Filters):ReturnType<typeof reportData>;catalog(actor:WorkContext):ReturnType<typeof metricCatalogFor>;metric(actor:WorkContext,metric:MetricId,filters:Filters,breakdown?:Breakdown):Promise<MetricData>};
export type ExecutiveToolPrincipal={db:ExecutiveMcpUnit;actor:WorkContext};
export function createExecutiveTools(ports:{clock:Clock;bytes(value:string):number;defaultFilters:typeof defaultFilters;error(status:number,code:string,message:string):Error;errorStatus(error:unknown):number|undefined}){
/** Trim grouped rows only, retaining exact totals, units, statuses and provenance for every metric. */
function boundedExecutiveResult(data:Record<string,unknown>):Record<string,unknown> {
  const bytes=(value:unknown)=>ports.bytes(JSON.stringify(value));
  if(bytes(data)<=TOOL_RESULT_BYTES)return data;
  const copy:Record<string,unknown>={...structuredClone(data),truncated:true,truncationReason:"Grouped rows were limited to keep the result within 128 KiB. Totals remain complete. Use query_metric for paginated groups within your own KPI permissions."};
  const groups:ReturnType<typeof metricProjection>[]=[];
  const collect=(v:unknown)=>{
    if(!v||typeof v!=="object")return;
    const x=v as Record<string,unknown>;
    if(x.schemaVersion===2&&Array.isArray(x.rows)&&x.pagination)groups.push(x as ReturnType<typeof metricProjection>);
    else for(const child of Object.values(x))collect(child);
  };
  collect(copy);
  while(bytes(copy)>TOOL_RESULT_BYTES) {
    const largest=groups.filter(g=>g.rows.length>0).sort((a,b)=>b.rows.length-a.rows.length)[0];
    if(!largest)throw ports.error(413,"RESULT_TOO_LARGE","The result metadata exceeds the MCP size limit. Choose a smaller dashboard.");
    largest.rows.splice(Math.floor(largest.rows.length/2));
    largest.pagination.returned=largest.rows.length;
    largest.pagination.truncated=true;
    largest.pagination.nextCursor=largest.rows.length?String(largest.pagination.offset+largest.rows.length):null;
  }
  return copy;
}
async function recheckDashboard(p:ExecutiveToolPrincipal,id:string,version:number,shareVersion?:number) {
  const fresh=await p.db.access(p.actor,id);
  if(fresh.r.version!==version||fresh.share?.version!==shareVersion)throw ports.error(409,"REPORT_CHANGED","The dashboard or its share changed while reading. Retry with current access.");
}
async function executeExecutiveTool(p:ExecutiveToolPrincipal,name:ToolName,raw:unknown):Promise<Record<string,unknown>> {
  const {db,actor}=p;
  if(name==="list_dashboards") {
    const args=executiveToolSchemas.list_dashboards.parse(raw);
    const ids=(await db.dashboards(actor,args.cursor??"",ports.clock.now(),args.limit+1)).results;
    const dashboards=[];
    const snapshots=[];
    for(const {id} of ids.slice(0,args.limit)) {
      let x;try{x=await db.access(actor,id);}catch(e){if([403,404].includes(ports.errorStatus(e)??0))continue;throw e;}
      dashboards.push({id,title:x.definition.title,description:x.definition.description,version:x.r.version,shareVersion:x.share?.version??0,owner:x.owner,shared:!!x.share,widgets:x.definition.widgets.length,updatedAt:x.r.updated_at});
      snapshots.push({id,version:x.r.version,shareVersion:x.share?.version});
    }
    for(const x of snapshots)await recheckDashboard(p,x.id,x.version,x.shareVersion);
    return {dashboards,scope:{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId},truncated:ids.length>args.limit,nextCursor:ids.length>args.limit?ids[args.limit-1].id:null};
  }
  if(name==="get_dashboard") {
    const args=executiveToolSchemas.get_dashboard.parse(raw),x=await db.access(actor,args.dashboardId);
    const result={id:args.dashboardId,title:x.definition.title,description:x.definition.description,filters:x.filters,lockedFilters:!!x.share,version:x.r.version,shareVersion:x.share?.version??0,mode:x.share?.mode??"owner",widgets:x.definition.widgets.map(widgetProjection),updatedAt:x.r.updated_at};
    await recheckDashboard(p,args.dashboardId,x.r.version,x.share?.version);
    return result;
  }
  if(name==="get_dashboard_data") {
    const args=executiveToolSchemas.get_dashboard_data.parse(raw),access=await db.access(actor,args.dashboardId);
    const filters=access.share?args.filters&&args.filters.currency===access.filters.currency&&args.filters.from>=access.filters.from&&args.filters.to<=access.filters.to?args.filters:access.filters:args.filters??access.filters;
    const data=await db.data(actor,args.dashboardId,args.filters);
    const map=(m:Record<string,MetricData>)=>Object.fromEntries(Object.entries(m).map(([id,value])=>[id,metricProjection(value,args.rowLimit)]));
    await recheckDashboard(p,args.dashboardId,access.r.version,access.share?.version);
    return {id:args.dashboardId,filters,metrics:map(data.metrics),comparisons:map(data.comparisons),updatedAt:data.updatedAt,version:data.version,shareVersion:data.shareVersion,mode:data.mode};
  }
  if(name==="list_metrics") {
    executiveToolSchemas.list_metrics.parse(raw);
    // Retain unavailable/pending status and reasons without offering unauthorized source data.
    return {metrics:await db.catalog(actor),defaultFilters:ports.defaultFilters(new Date(ports.clock.now())),scope:{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId}};
  }
  const args=executiveToolSchemas.query_metric.parse(raw);
  const offset=args.cursor===undefined?0:/^(0|[1-9]\d{0,7})$/.test(args.cursor)?Number(args.cursor):NaN;
  if(!Number.isSafeInteger(offset))throw ports.error(422,"CURSOR_INVALID","Use the nextCursor returned by query_metric.");
  return {filters:args.filters,breakdown:args.breakdown,data:metricProjection(await db.metric(actor,args.metric,args.filters,args.breakdown),args.limit,offset)};
}


return {boundedExecutiveResult,executeExecutiveTool};}
