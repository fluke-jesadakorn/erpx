import "server-only";
import {Agent, request} from "node:http";
import {ApiError} from "@erpx/server-runtime/public/http";

/**
 * Strict private transport for the native Executive Quality inspection search.
 * The axum owner executes the SQL, projects the seven-field row and enforces
 * the Company scope. This bridge emits no SQL, accepts no SQL and keeps no
 * local fallback reader: missing, malformed or failing native service fails
 * closed with ApiError(503).
 */
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
}>;
type Config = Readonly<{base:string;timeoutMs:number;holdings:readonly Holding[]}>;

export type NativeQualityEvidenceRow = Readonly<{
  id:string;
  hold_id:string;
  result:"pass"|"fail";
  created_at:number;
  reviewed_version:number;
  product_name:string;
  warehouse_name:string;
}>;

export type NativeQualityEvidenceActor = Readonly<{
  holdingId:string;
  holdingSlug:string;
  companyId:string;
  branchId?:string|null;
}>;

export type NativeQualityEvidenceFilters = Readonly<{
  from:string;
  to:string;
  currency:string;
  query:string;
  limit:number;
  /** The retained base64url `qualityCursorValue` envelope, verbatim. */
  cursor?:string|null;
}>;

const ENV = "ERPX_NATIVE_QUALITY_EVIDENCE_CONFIG";
const RESERVED = new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
const ROW_KEYS = ["id","hold_id","result","created_at","reviewed_version","product_name","warehouse_name"] as const;
const unavailable=(code="NATIVE_QUALITY_EVIDENCE_UNAVAILABLE",status=503):never=>{
 throw new ApiError(status,code,"The native Quality evidence service is unavailable.");
};
const object=(value:unknown):value is Record<string,unknown>=>value!==null&&typeof value==="object"&&!Array.isArray(value);
const text=(value:unknown,max=4096):value is string=>typeof value==="string"&&new TextEncoder().encode(value).length<=max;
const validSlug=(value:unknown):value is string=>text(value,128)&&/^[a-z0-9_-]+$/.test(value as string);
const validId=(value:unknown):value is string=>text(value,200)&&/^[A-Za-z0-9._:-]+$/.test(value as string);
const validHost=(value:unknown):value is string=>text(value,253)&&/^[a-z0-9.-]+$/.test(value as string);
const date=(value:unknown):value is string=>text(value,10)&&/^\d{4}-\d{2}-\d{2}$/.test(value as string);

export function nativeQualityEvidenceConfiguration(raw:string|undefined):Config{
 if(raw===undefined)return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_REQUIRED");
 let value:unknown;
 try{value=JSON.parse(raw);}catch{return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");}
 if(!object(value)||!text(value.privateBaseUrl,512)||typeof value.timeoutMs!=="number"||
   !Number.isSafeInteger(value.timeoutMs)||value.timeoutMs<250||value.timeoutMs>30000||
   !Array.isArray(value.holdings)||value.holdings.length<1||value.holdings.length>64)
  return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");
 let base:URL;
 try{base=new URL(value.privateBaseUrl);}catch{return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");}
 const port=Number(base.port);
 if(base.protocol!=="http:"||!["127.0.0.1","[::1]"].includes(base.hostname)||!base.port||
   RESERVED.has(port)||(port>=3200&&port<=3299)||base.username||base.password||base.origin!==value.privateBaseUrl)
  return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");
 const holdings:Holding[]=[],ids=new Set<string>();
 for(const item of value.holdings){
  if(!object(item)||!text(item.origin,512)||!validHost(item.host)||!validId(item.holdingId)||!validSlug(item.holdingSlug))
   return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");
  let origin:URL;
  try{origin=new URL(item.origin);}catch{return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");}
  if(origin.protocol!=="https:"||origin.origin!==item.origin||origin.username||origin.password||origin.host!==item.host)
   return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");
  if(ids.has(item.holdingId as string)||holdings.some(h=>h.host===item.host&&h.holdingSlug===item.holdingSlug))
   return unavailable("NATIVE_QUALITY_EVIDENCE_CONFIGURATION_INVALID");
  ids.add(item.holdingId as string);
  holdings.push({origin:origin.origin,host:item.host as string,holdingId:item.holdingId as string,holdingSlug:item.holdingSlug as string});
 }
 return {base:base.origin,timeoutMs:value.timeoutMs,holdings};
}

function validRow(value:unknown):value is NativeQualityEvidenceRow{
 if(!object(value))return false;
 const keys=Object.keys(value);
 if(keys.length!==ROW_KEYS.length||!ROW_KEYS.every(key=>key in value))return false;
 if(!text(value.id,200)||!text(value.hold_id,200)||!text(value.product_name,500)||!text(value.warehouse_name,500))return false;
 if(value.result!=="pass"&&value.result!=="fail")return false;
 return [value.created_at,value.reviewed_version].every(v=>typeof v==="number"&&Number.isSafeInteger(v));
}

function validRequest(actor:NativeQualityEvidenceActor,filters:NativeQualityEvidenceFilters):boolean{
 return validId(actor.holdingId)&&validSlug(actor.holdingSlug)&&validId(actor.companyId)&&
  (actor.branchId===undefined||actor.branchId===null||validId(actor.branchId))&&
  date(filters.from)&&date(filters.to)&&filters.from<=filters.to&&
  /^[A-Z]{3}$/.test(filters.currency)&&text(filters.query,100)&&
  Number.isSafeInteger(filters.limit)&&filters.limit>=1&&filters.limit<=50&&
  (filters.cursor===undefined||filters.cursor===null||text(filters.cursor,2000));
}

/** POST with an explicit Host header; fetch forbids overriding Host. */
function post(url:URL,headers:Record<string,string>,body:string,timeoutMs:number):Promise<{status:number;value:unknown}>{
 return new Promise((resolve,reject)=>{
  const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import("node:http").ProxyEnv});
  const outgoing=request(url,{method:"POST",agent,headers:{...headers,"content-length":String(Buffer.byteLength(body))},signal:AbortSignal.timeout(timeoutMs),maxHeaderSize:8192},incoming=>{
   const chunks:Buffer[]=[];
   incoming.on("data",chunk=>chunks.push(Buffer.from(chunk)));
   incoming.on("end",()=>{
    agent.destroy();
    const raw=Buffer.concat(chunks).toString("utf8");
    let value:unknown=null;try{value=raw?JSON.parse(raw):null;}catch{value=null;}
    resolve({status:incoming.statusCode??502,value});
   });
   incoming.on("error",error=>{agent.destroy();reject(error);});
  });
  outgoing.on("error",error=>{agent.destroy();reject(error);});
  outgoing.end(body);
 });
}

/** Request the bounded, Company-scoped page from the native owner. */
export async function nativeQualityEvidenceRows(
 actor:NativeQualityEvidenceActor,
 filters:NativeQualityEvidenceFilters,
):Promise<readonly NativeQualityEvidenceRow[]>{
 const config=nativeQualityEvidenceConfiguration(process.env[ENV]);
 if(!validRequest(actor,filters))return unavailable("NATIVE_QUALITY_EVIDENCE_REQUEST_INVALID",422);
 const target=config.holdings.find(h=>h.holdingId===actor.holdingId&&h.holdingSlug===actor.holdingSlug);
 if(!target)return unavailable("WORK_CONTEXT_CHANGED",409);
 const envelope:Record<string,unknown>={
  metric:"inspection-pass-rate",
  filters:{from:filters.from,to:filters.to,currency:filters.currency},
  query:filters.query,
  cursor:filters.cursor??null,
  limit:filters.limit,
 };
 const body=JSON.stringify(envelope);
 if(new TextEncoder().encode(body).length>16*1024)return unavailable("NATIVE_QUALITY_EVIDENCE_REQUEST_INVALID",422);
 const url=new URL(`/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/quality/evidence/inspections`,config.base);
 let response:{status:number;value:unknown};
 try{
  response=await post(url,{
   host:target.host,
   accept:"application/json",
   "content-type":"application/json",
   "cache-control":"no-cache",
   "x-erpx-context-company":actor.companyId,
   ...(actor.branchId?{"x-erpx-workspace-branch":actor.branchId}:{}),
  },body,config.timeoutMs);
 }catch{return unavailable();}
 if(response.status!==200){
  const error=object(response.value)&&object(response.value.error)?response.value.error:undefined;
  if(response.status>=400&&response.status<500)throw new ApiError(
   response.status,
   text(error?.code,100)?String(error!.code):"NATIVE_QUALITY_EVIDENCE_READ_FAILED",
   text(error?.message,300)?String(error!.message):"The native Quality evidence read was rejected.",
  );
  return unavailable();
 }
 const rows=object(response.value)?response.value.rows:undefined;
 if(!Array.isArray(rows)||rows.length>51||!rows.every(validRow))return unavailable("NATIVE_QUALITY_EVIDENCE_RESPONSE_INVALID");
 return rows as NativeQualityEvidenceRow[];
}

/** True when the native Quality evidence consumer switch is configured. */
export function nativeQualityEvidenceConfigured(environment:Readonly<Record<string,string|undefined>>=process.env):boolean{
 return environment[ENV]!==undefined;
}
