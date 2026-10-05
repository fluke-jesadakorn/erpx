import "server-only";
import { randomUUID } from "node:crypto";
import { createMcpHandler, McpServer, type CallToolResult } from "@modelcontextprotocol/server";
import { z } from "zod";
import { breakdowns, defaultFilters, filtersSchema, metricIds, type MetricData, type Widget } from "@/lib/executive/canvas";
import type { SqlDatabase } from "../runtime/types";
import { runtimeEnv } from "../runtime";
import { ApiError } from "../http";
import { requestPublicOrigin } from "../public-origin";
import { consumeExecutiveRateLimit, findExecutiveToken, recheckExecutivePrincipal, resolveExecutivePrincipal, type ExecutivePrincipal, type TokenRow } from "./connections";
import { reportAccess, reportData } from "./reports";
import { metricCatalogFor, queryMetric } from "./metrics";

const page = { limit:z.number().int().min(1).max(200).default(100), cursor:z.string().max(180).optional() };
const dashboardId = z.string().min(1).max(180);
export const executiveToolSchemas = {
  list_dashboards:z.object(page).strict(),
  get_dashboard:z.object({dashboardId}).strict(),
  get_dashboard_data:z.object({dashboardId,filters:filtersSchema.optional(),rowLimit:z.number().int().min(1).max(200).default(100)}).strict(),
  list_metrics:z.object({}).strict(),
  query_metric:z.object({metric:z.enum(metricIds),filters:filtersSchema,breakdown:z.enum(breakdowns).default("default"),...page}).strict(),
};
type ToolName = keyof typeof executiveToolSchemas;
const TOOL_RESULT_BYTES = 128*1024;
const BODY_BYTES = 32*1024;
const annotations = {readOnlyHint:true,destructiveHint:false,idempotentHint:true,openWorldHint:false};
const widgetProjection = (w:Widget) => ({id:w.id,title:w.title,description:w.description,metric:w.metric,kind:w.kind,breakdown:w.breakdown??"default",comparePeriod:w.comparePeriod??null});
export function metricProjection(data:MetricData,limit:number,offset=0) {
  return {...data,valueAvailable:!["unavailable","failed"].includes(data.status),rows:data.rows.slice(offset,offset+limit),pagination:{offset,returned:Math.min(limit,Math.max(0,data.rows.length-offset)),totalRows:data.rows.length,truncated:offset+limit<data.rows.length,nextCursor:offset+limit<data.rows.length?String(offset+limit):null}};
}
/** Trim grouped rows only, retaining exact totals, units, statuses and provenance for every metric. */
export function boundedExecutiveResult(data:Record<string,unknown>):Record<string,unknown> {
  const bytes=(value:unknown)=>Buffer.byteLength(JSON.stringify(value));
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
    if(!largest)throw new ApiError(413,"RESULT_TOO_LARGE","The result metadata exceeds the MCP size limit. Choose a smaller dashboard.");
    largest.rows.splice(Math.floor(largest.rows.length/2));
    largest.pagination.returned=largest.rows.length;
    largest.pagination.truncated=true;
    largest.pagination.nextCursor=largest.rows.length?String(largest.pagination.offset+largest.rows.length):null;
  }
  return copy;
}
async function recheckDashboard(p:ExecutivePrincipal,id:string,version:number,shareVersion?:number) {
  const fresh=await reportAccess(p.db,p.actor,id);
  if(fresh.r.version!==version||fresh.share?.version!==shareVersion)throw new ApiError(409,"REPORT_CHANGED","The dashboard or its share changed while reading. Retry with current access.");
}
export async function executeExecutiveTool(p:ExecutivePrincipal,name:ToolName,raw:unknown):Promise<Record<string,unknown>> {
  const {db,actor}=p;
  if(name==="list_dashboards") {
    const args=executiveToolSchemas.list_dashboards.parse(raw);
    const ids=(await db.prepare(`SELECT r.id FROM executive_reports r WHERE r.company_id=? AND r.id>?
      AND (r.owner_id=? OR EXISTS(SELECT 1 FROM executive_report_shares s WHERE s.report_id=r.id AND s.company_id=r.company_id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?)))
      ORDER BY r.id LIMIT ?`).bind(actor.companyId,args.cursor??"",actor.userId,actor.userId,Date.now(),args.limit+1).all<{id:string}>()).results;
    const dashboards=[];
    const snapshots=[];
    for(const {id} of ids.slice(0,args.limit)) {
      let x;try{x=await reportAccess(db,actor,id);}catch(e){if(e instanceof ApiError&&[403,404].includes(e.status))continue;throw e;}
      dashboards.push({id,title:x.definition.title,description:x.definition.description,version:x.r.version,shareVersion:x.share?.version??0,owner:x.owner,shared:!!x.share,widgets:x.definition.widgets.length,updatedAt:x.r.updated_at});
      snapshots.push({id,version:x.r.version,shareVersion:x.share?.version});
    }
    for(const x of snapshots)await recheckDashboard(p,x.id,x.version,x.shareVersion);
    return {dashboards,scope:{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId},truncated:ids.length>args.limit,nextCursor:ids.length>args.limit?ids[args.limit-1].id:null};
  }
  if(name==="get_dashboard") {
    const args=executiveToolSchemas.get_dashboard.parse(raw),x=await reportAccess(db,actor,args.dashboardId);
    const result={id:args.dashboardId,title:x.definition.title,description:x.definition.description,filters:x.filters,lockedFilters:!!x.share,version:x.r.version,shareVersion:x.share?.version??0,mode:x.share?.mode??"owner",widgets:x.definition.widgets.map(widgetProjection),updatedAt:x.r.updated_at};
    await recheckDashboard(p,args.dashboardId,x.r.version,x.share?.version);
    return result;
  }
  if(name==="get_dashboard_data") {
    const args=executiveToolSchemas.get_dashboard_data.parse(raw),access=await reportAccess(db,actor,args.dashboardId);
    const filters=access.share?args.filters&&args.filters.currency===access.filters.currency&&args.filters.from>=access.filters.from&&args.filters.to<=access.filters.to?args.filters:access.filters:args.filters??access.filters;
    const data=await reportData(db,actor,args.dashboardId,args.filters);
    const map=(m:Record<string,MetricData>)=>Object.fromEntries(Object.entries(m).map(([id,value])=>[id,metricProjection(value,args.rowLimit)]));
    await recheckDashboard(p,args.dashboardId,access.r.version,access.share?.version);
    return {id:args.dashboardId,filters,metrics:map(data.metrics),comparisons:map(data.comparisons),updatedAt:data.updatedAt,version:data.version,shareVersion:data.shareVersion,mode:data.mode};
  }
  if(name==="list_metrics") {
    executiveToolSchemas.list_metrics.parse(raw);
    // Retain unavailable/pending status and reasons without offering unauthorized source data.
    return {metrics:await metricCatalogFor(db,actor),defaultFilters:defaultFilters(),scope:{companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId}};
  }
  const args=executiveToolSchemas.query_metric.parse(raw);
  const offset=args.cursor===undefined?0:/^(0|[1-9]\d{0,7})$/.test(args.cursor)?Number(args.cursor):NaN;
  if(!Number.isSafeInteger(offset))throw new ApiError(422,"CURSOR_INVALID","Use the nextCursor returned by query_metric.");
  return {filters:args.filters,breakdown:args.breakdown,data:metricProjection(await queryMetric(db,actor,args.metric,args.filters,args.breakdown),args.limit,offset)};
}

async function readBody(request:Request) {
  if(!request.headers.get("content-type")?.toLowerCase().startsWith("application/json"))throw new ApiError(415,"CONTENT_TYPE_REQUIRED","Use application/json.");
  const reader=request.body?.getReader();if(!reader)throw new ApiError(400,"INVALID_JSON","A JSON-RPC body is required.");
  let bytes=0,text="";const decoder=new TextDecoder();
  while(true){const chunk=await reader.read();if(chunk.done)break;bytes+=chunk.value.byteLength;if(bytes>BODY_BYTES){await reader.cancel();throw new ApiError(413,"BODY_TOO_LARGE","MCP requests are limited to 32 KiB.");}text+=decoder.decode(chunk.value,{stream:true});}
  try{return JSON.parse(text+decoder.decode()) as unknown;}catch{throw new ApiError(400,"INVALID_JSON","Use a valid JSON-RPC request.");}
}
function failure(error:unknown) {
  return error instanceof ApiError ? error : new ApiError(503,"EXECUTIVE_UNAVAILABLE","Executive MCP is temporarily unavailable. Retry later.");
}
function httpFailure(e:ApiError) {
  return Response.json({jsonrpc:"2.0",id:null,error:{code:-32000,message:e.message,data:{code:e.code}}},{status:e.status,headers:{"Cache-Control":"private, no-store",...(e.status===401?{"WWW-Authenticate":'Bearer realm="ERPX Executive"'}:{}),...(e.status===429?{"Retry-After":String(60-Math.floor(Date.now()/1000)%60)}:{})}});
}
export async function handleExecutiveMcp(request:Request) {
  const started=Date.now();let control:SqlDatabase|undefined,token:TokenRow|undefined,action="request",outcome:"success"|"denied"|"failed"="success",code="OK";
  let response:Response;
  try {
    control=(await runtimeEnv()).CONTROL_DB;
    const origin=request.headers.get("origin");
    if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","This MCP origin is not allowed.");
    token=await findExecutiveToken(control,request.headers.get("authorization"));
    // Expired/revoked attempts still have attribution, but never reach a business read.
    const p=await resolveExecutivePrincipal(control,token);
    await consumeExecutiveRateLimit(control,token);
    if(request.method!=="POST")throw new ApiError(405,"METHOD_NOT_ALLOWED","Use Streamable HTTP POST. This endpoint holds no sessions or subscription streams.");
    const raw=await readBody(request);
    if(!raw||typeof raw!=="object"||Array.isArray(raw))throw new ApiError(400,"INVALID_REQUEST","Send one JSON-RPC request at a time.");
    const rpc=raw as {method?:unknown;params?:{name?:unknown;arguments?:unknown}};
    const toolName=rpc.method==="tools/call"&&typeof rpc.params?.name==="string"&&Object.hasOwn(executiveToolSchemas,rpc.params.name)?rpc.params.name as ToolName:undefined;
    action=toolName??(["initialize","server/discover","tools/list","ping","notifications/initialized"].includes(String(rpc.method))?String(rpc.method):"unknown");
    if(rpc.method==="tools/call"&&(!toolName||!executiveToolSchemas[toolName].safeParse(rpc.params?.arguments??{}).success)){outcome="denied";code="INVALID_TOOL_ARGUMENTS";}
    const handler=createMcpHandler(()=>{
      const server=new McpServer({name:"erpx-executive",version:"1.0.0"});
      const register=(name:ToolName,description:string,schema:z.ZodObject)=>server.registerTool(name,{description,inputSchema:schema,annotations},async(args:unknown):Promise<CallToolResult>=>{
        try {
          await recheckExecutivePrincipal(control!,p);
          const result=boundedExecutiveResult(await executeExecutiveTool(p,name,args));
          await recheckExecutivePrincipal(control!,p);
          return {content:[{type:"text",text:JSON.stringify(result)}],structuredContent:result};
        }catch(error){const e=failure(error);outcome=e.status>=500?"failed":"denied";code=e.code;return {isError:true,content:[{type:"text",text:JSON.stringify({error:{code:e.code,message:e.message}})}]};}
      });
      register("list_dashboards","List owned and currently shared Executive dashboards. Use nextCursor to continue.",executiveToolSchemas.list_dashboards);
      register("get_dashboard","Read dashboard title, approved filters and analytical widget metadata.",executiveToolSchemas.get_dashboard);
      register("get_dashboard_data","Read exact dashboard metrics and comparisons using existing recipient/distribution share rules. Shared dates can only be narrowed. When valueAvailable is false, total is a contract sentinel; never interpret it as a measured zero.",executiveToolSchemas.get_dashboard_data);
      register("list_metrics","List Executive KPI definitions, supported breakdowns, availability and pending acceptance for the token's current role and scope.",executiveToolSchemas.list_metrics);
      register("query_metric","Read an authorized KPI for a date interval and supported breakdown. Exact integer totals are decimal strings. A report share does not grant additional KPI access.",executiveToolSchemas.query_metric);
      return server;
    },{responseMode:"auto",maxRequestBodySize:BODY_BYTES,maxSubscriptions:0});
    try {
      const result=await handler.fetch(request,{parsedBody:raw});
      const body=await result.text(); // Finish the SDK stream before disposing the per-request server.
      if(Buffer.byteLength(body)>2*TOOL_RESULT_BYTES+8192)throw new ApiError(413,"RESULT_TOO_LARGE","The MCP response exceeds its size limit.");
      await recheckExecutivePrincipal(control,p);
      if(result.status>=400){outcome="denied";code="PROTOCOL_REJECTED";}
      else if(outcome==="success"&&body){
        // SDK validation failures may use HTTP 200 JSON or legacy SSE; audit the protocol outcome too.
        const wire=result.headers.get("content-type")?.includes("text/event-stream")?body.split("\n").filter(line=>line.startsWith("data: ")).at(-1)?.slice(6):body;
        if(wire)try{const message=JSON.parse(wire);if(message.error||message.result?.isError){outcome="denied";code="PROTOCOL_REJECTED";}}catch{/* Non-result notifications carry no business data. */}
      }
      response=new Response(body||null,{status:result.status,headers:result.headers});
      response.headers.set("Cache-Control","private, no-store");
      await control.prepare("UPDATE executive_external_tokens SET last_used_at=? WHERE id=? AND revoked_at IS NULL").bind(Date.now(),token.id).run();
    }finally{await handler.close();}
  }catch(error){const e=failure(error);outcome=e.status>=500?"failed":"denied";code=e.code;response=httpFailure(e);}
  try {
    if(!control)throw new Error("CONTROL_UNAVAILABLE");
    await control.prepare(`INSERT INTO executive_mcp_audit(id,token_id,user_id,company_id,assignment_id,branch_id,department_id,action,outcome,code,started_at,duration_ms)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`).bind(randomUUID(),token?.id??null,token?.user_id??null,token?.company_id??null,token?.assignment_id??null,token?.branch_id??null,token?.department_id??null,action,outcome,code,started,Date.now()-started).run();
  }catch{return httpFailure(new ApiError(503,"EXECUTIVE_UNAVAILABLE","Executive MCP is temporarily unavailable. Retry later."));}
  return response;
}
