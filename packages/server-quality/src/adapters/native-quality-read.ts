import "server-only";
import {Agent, request as httpRequest} from "node:http";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {SqlDatabase,SqlResult,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";

/** Allow-listed read kinds resolved by the mounted `/quality/read-plan` route. */
export const RD_ALERT_HOLD_ID = "quality_rd_alert_hold_id";
export const RD_HOLD_EXISTS = "quality_rd_hold_exists";
export const RD_SPEC_EXISTS = "quality_rd_spec_exists";
export const RD_ATTACH_HOLD_CAS = "quality_rd_attach_hold_cas";
export const RD_ATTACH_SPEC_CAS = "quality_rd_attach_spec_cas";

/**
 * Strict owner transport for the native Quality read-plan route. The axum
 * owner resolves a closed kind and positional values to one exact static
 * statement; this bridge emits no SQL, accepts none from a caller and executes
 * the returned statement on the caller's already-open unit. Missing
 * configuration or any failed configured request fails closed with no
 * TypeScript fallback.
 */
type Holding = Readonly<{origin:string;holdingSlug:string}>;
type Settings = Readonly<{base:string;timeoutMs:number;holding:Holding}>;

const reservedPorts = new Set([3000,3001,3100,3101,3103,5432,5433,5435,8080]);
const record = (v:unknown):v is Record<string,unknown> => v!==null && typeof v==="object" && !Array.isArray(v);

const fail = (code:string,status=503,message="The native Quality read service is unavailable."):never => {
 throw new ApiError(status,code,message);
};

function config():Settings {
 const raw=process.env.ERPX_NATIVE_QUALITY_CONFIG;
 if(raw===undefined)return fail("QUALITY_NATIVE_READ_UNAVAILABLE");
 let value:unknown;
 try{value=JSON.parse(raw);}catch{return fail("QUALITY_NATIVE_READ_CONFIG_INVALID");}
 if(!record(value)||typeof value.privateBaseUrl!=="string"||!Number.isSafeInteger(value.timeoutMs)||Number(value.timeoutMs)<250||Number(value.timeoutMs)>30000||!Array.isArray(value.holdings)||value.holdings.length<1||value.holdings.length>64)return fail("QUALITY_NATIVE_READ_CONFIG_INVALID");
 let base:URL;
 try{base=new URL(value.privateBaseUrl);}catch{return fail("QUALITY_NATIVE_READ_CONFIG_INVALID");}
 const port=Number(base.port);
 if(base.protocol!=="http:"||!["127.0.0.1","[::1]"].includes(base.hostname)||!base.port||reservedPorts.has(port)||(port>=3200&&port<=3299)||base.origin!==value.privateBaseUrl||base.username||base.password)return fail("QUALITY_NATIVE_READ_CONFIG_INVALID");
 const holding=value.holdings[0];
 if(!record(holding)||typeof holding.origin!=="string"||typeof holding.holdingSlug!=="string"||!/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug))return fail("QUALITY_NATIVE_READ_CONFIG_INVALID");
 let origin:URL;
 try{origin=new URL(holding.origin);}catch{return fail("QUALITY_NATIVE_READ_CONFIG_INVALID");}
 if(origin.protocol!=="https:"||origin.origin!==holding.origin||origin.username||origin.password)return fail("QUALITY_NATIVE_READ_CONFIG_INVALID");
 return {base:base.origin,timeoutMs:Number(value.timeoutMs),holding:{origin:origin.origin,holdingSlug:holding.holdingSlug}};
}

/** POST with an explicit Host header; fetch forbids overriding Host. */
function post(url:URL,headers:Record<string,string>,body:string,timeoutMs:number):Promise<{status:number;value:unknown}>{
 return new Promise((resolve,reject)=>{
  const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import("node:http").ProxyEnv});
  const outgoing=httpRequest(url,{method:"POST",agent,headers:{...headers,"content-length":String(Buffer.byteLength(body))},signal:AbortSignal.timeout(timeoutMs),maxHeaderSize:8192},incoming=>{
   const chunks:Buffer[]=[];
   incoming.on("data",chunk=>chunks.push(Buffer.from(chunk)));
   incoming.on("end",()=>{
    agent.destroy();
    const text=Buffer.concat(chunks).toString("utf8");
    let value:unknown=null;try{value=text?JSON.parse(text):null;}catch{value=null;}
    resolve({status:incoming.statusCode??502,value});
   });
   incoming.on("error",error=>{agent.destroy();reject(error);});
  });
  outgoing.on("error",error=>{agent.destroy();reject(error);});
  outgoing.end(body);
 });
}

type Planned = Readonly<{sql:string;values:readonly unknown[]}>;

/** Ask the owner to plan one read; only the kind and positional values cross
 * the wire. The reply is validated to carry exactly one owner statement. */
async function plan(kind:string,values:readonly unknown[]):Promise<Planned>{
 const cfg=config();
 let response:{status:number;value:unknown};
 try{
  response=await post(new URL(`/api/v1/holdings/${encodeURIComponent(cfg.holding.holdingSlug)}/quality/read-plan`,cfg.base),{
   host:new URL(cfg.holding.origin).host,
   origin:cfg.holding.origin,
   accept:"application/json",
   "content-type":"application/json",
  },JSON.stringify({kind,values}),cfg.timeoutMs);
 }catch{return fail("QUALITY_NATIVE_READ_UNAVAILABLE");}
 const value:unknown=response.value;
 if(response.status!==200){
  const error=record(value)&&record(value.error)?value.error:{};
  throw new ApiError(response.status,typeof error.code==="string"?error.code:"QUALITY_NATIVE_READ_FAILED",typeof error.message==="string"?error.message:"The owned Quality read could not be planned.");
 }
 const statements=record(value)&&record(value.data)?value.data.statements:undefined;
 if(!Array.isArray(statements)||statements.length!==1)return fail("QUALITY_NATIVE_READ_RESPONSE_INVALID");
 const entry=statements[0];
 if(!record(entry)||typeof entry.sql!=="string"||!Array.isArray(entry.values)||entry.values.length>64||entry.values.length!==entry.sql.split("?").length-1)return fail("QUALITY_NATIVE_READ_RESPONSE_INVALID");
 return {sql:entry.sql,values:entry.values as readonly unknown[]};
}

/** Return the owner-planned SQL text and positional values to a trusted server
 * caller that composes a correlated fragment on its already-open unit. The
 * statement is selected by the Rust allow-list; no SQL is accepted from a
 * caller and no TypeScript mirror is emitted. */
export async function nativeQualityReadSql(kind:string,values:readonly unknown[]):Promise<Planned>{
 return plan(kind,values);
}

/** Planned owner read returning a single row (retired `.first()` parity). */
export async function nativeQualityReadFirst<T>(kind:string,values:readonly unknown[],db:SqlDatabase):Promise<T|null>{
 const statement=await nativeQualityReadSql(kind,values);
 return db.prepare(statement.sql).bind(...(statement.values as unknown[])).first<T>();
}

type GuardContext = Readonly<{companyId?:string;db?:SqlDatabase;now?:number;key?:string}>;

/** One closed attachment CAS guard as a native write descriptor. Planning is
 * deferred to run() time: the Workflow batch boundary attaches its caller
 * context first, then this descriptor asks the Quality owner for the exact
 * guard statement and delegates to the unchanged Workflow assertion on that
 * same open unit. No guard SQL is ever composed in TypeScript. */
export function nativeQualityGuardAssertion(
 db:SqlDatabase,
 commandId:string,
 step:string,
 kind:string,
 values:readonly unknown[],
):SqlStatement{
 let context:GuardContext|undefined;
 const execute=async():Promise<SqlResult<Record<string,unknown>>>=>{
  const statement=await nativeQualityReadSql(kind,values);
  const guard=assertion(db,commandId,step,statement.sql,statement.values as (string|number|null)[]);
  return (context?guard.withContext(context):guard).run();
 };
 const descriptor={
  __nativeWrite:true,
  kind,
  values,
  withContext(next:GuardContext){context=next;return descriptor;},
  bind(){return descriptor;},
  run(){return execute();},
  async first<T>(){await execute();return null as T|null;},
  async all<T>(){await execute();return {results:[] as T[],success:true,meta:{changes:0,duration:0}} as SqlResult<T>;},
  async raw<T>(){return [] as T[];},
 };
 return descriptor as unknown as SqlStatement;
}
