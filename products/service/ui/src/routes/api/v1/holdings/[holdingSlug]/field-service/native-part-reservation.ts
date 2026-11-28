import {contributorExactOrigin as exactOrigin,holdingHostForHost as exactHolding} from '@erpx/platform-contracts/holding-hosts';
import {allowedPrivateLoopbackPort} from '@erpx/platform-native-sdk/loopback';
import {Agent, request as httpRequest} from 'node:http';
import {Readable} from 'node:stream';
import 'server-only';
// Strict fail-closed transport for the mounted native Field Service part-reservation read.
// The owner axum router executes the SQL and projects the exact eight-field
// reservation object. This bridge emits no SQL and has no fallback reader: a
// missing or invalid config, an unmatched Holding, a transport failure or a
// drifted reply all return a closed error.
type Settings={privateBaseUrl:string;timeoutMs:number;holdings:{origin:string;holdingSlug:string;holdingId:string}[];production?:boolean};
const headers={'Cache-Control':'private, no-store'};
const closed=(status:number,code:string,message:string)=>Response.json({error:{code,message}},{status,headers});
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const safe=(v:unknown,min:number):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=min;
const contributorOriginProfile=()=>process.env.NODE_ENV==='development'&&process.env.NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE==='container-localhost';
const contributorExactOrigin=(host:string|null|undefined)=>exactOrigin(host,contributorOriginProfile()?'contributor':'public');
const holdingHostForHost=(host:string|null|undefined)=>exactHolding(host,contributorOriginProfile()?'contributor':'public');
function settings(raw:string|undefined):Settings {
 if(raw===undefined)throw Error('NATIVE_SERVICE_UNAVAILABLE');
 let v:unknown;try{v=JSON.parse(raw);}catch{throw Error('NATIVE_SERVICE_CONFIGURATION_INVALID');}
 if(!v||typeof v!=='object'||Array.isArray(v))throw Error('NATIVE_SERVICE_CONFIGURATION_INVALID');
 const s=v as Settings;let base:URL;try{base=new URL(s.privateBaseUrl);}catch{throw Error('NATIVE_SERVICE_CONFIGURATION_INVALID');}
 const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||!allowedPrivateLoopbackPort(port,s.production===true)||port>=3200&&port<=3299||base.username||base.password||s.privateBaseUrl!==base.origin||!Number.isSafeInteger(s.timeoutMs)||s.timeoutMs<250||s.timeoutMs>30000||!Array.isArray(s.holdings)||s.holdings.length<1||s.holdings.length>64)throw Error('NATIVE_SERVICE_CONFIGURATION_INVALID');
 const hosts=new Set<string>();for(const h of s.holdings){let u:URL;try{u=new URL(h.origin);}catch{throw Error('NATIVE_SERVICE_CONFIGURATION_INVALID');}if(u.protocol!=='https:'||h.origin!==u.origin||(contributorOriginProfile()?(contributorExactOrigin(u.host)!==u.origin||holdingHostForHost(u.host)?.holdingId!==h.holdingId||holdingHostForHost(u.host)?.holdingSlug!==h.holdingSlug):!!u.port)||u.username||u.password||!u.hostname.includes('.')||!u.hostname.split('.').every(p=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p))||typeof h.holdingSlug!=='string'||!/^[a-z0-9_-]{1,128}$/.test(h.holdingSlug)||typeof h.holdingId!=='string'||!h.holdingId||new TextEncoder().encode(h.holdingId).length>200||hosts.has(u.host))throw Error('NATIVE_SERVICE_CONFIGURATION_INVALID');hosts.add(u.host);}return s;
}
async function bytes(stream:ReadableStream<Uint8Array>|null,max:number){if(!stream)return new Uint8Array();const reader=stream.getReader(),chunks:Uint8Array[]=[];let size=0;try{for(;;){const part=await reader.read();if(part.done)break;size+=part.value.length;if(size>max){await reader.cancel();throw Error('LIMIT');}chunks.push(part.value);}}finally{reader.releaseLock();}const out=new Uint8Array(size);let offset=0;for(const chunk of chunks){out.set(chunk,offset);offset+=chunk.length;}return out;}
function direct(url:URL,headers:Headers,signal:AbortSignal):Promise<Response>{return new Promise((resolve,reject)=>{const agent=new Agent({keepAlive:false,maxSockets:1,maxTotalSockets:1,proxyEnv:{} as import('node:http').ProxyEnv});const req=httpRequest(url,{method:'GET',headers:Object.fromEntries(headers),agent,signal,maxHeaderSize:8192},incoming=>{incoming.once('close',()=>agent.destroy());try{const h=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)h.append(incoming.rawHeaders[i]!,incoming.rawHeaders[i+1]!);resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>,{status:incoming.statusCode??502,headers:h}));}catch(e){incoming.destroy();agent.destroy();reject(e);}});req.once('error',e=>{agent.destroy();reject(e);});req.end();});}
const RESERVATION_KEYS=['consumedMilli','expiresAt','id','orderId','productId','quantityMilli','releasedMilli','warehouseId'];
function reservation(value:unknown,orderId:string,reservationId:string):value is Record<string,unknown>{
 if(!value||typeof value!=='object'||Array.isArray(value))return false;
 const p=value as Record<string,unknown>;
 if(Object.keys(p).sort().join(',')!==RESERVATION_KEYS.join(','))return false;
 return p.id===reservationId&&p.orderId===orderId&&text(p.warehouseId)&&new TextEncoder().encode(p.warehouseId).length<=1024&&text(p.productId)&&new TextEncoder().encode(p.productId).length<=1024
  &&safe(p.quantityMilli,1)&&safe(p.consumedMilli,0)&&safe(p.releasedMilli,0)&&Number(p.consumedMilli)+Number(p.releasedMilli)<=Number(p.quantityMilli)&&safe(p.expiresAt,1);
}
/** Returns only the mounted native reservation object; never a TS substitute. */
export async function nativePartReservationReadback(request:Request,slug:string,orderId:string,reservationId:string):Promise<Response>{
 const unavailable=()=>closed(503,'NATIVE_SERVICE_UNAVAILABLE','Unable to confirm the reservation.');
 let config:Settings;try{config=settings(process.env.ERPX_NATIVE_FIELD_SERVICE_RESERVE_CONFIG);}catch{return unavailable();}
 const host=request.headers.get('host'),target=config.holdings.find(h=>new URL(h.origin).host===host&&h.holdingSlug===slug);
 if(!target||!text(orderId)||!text(reservationId)||orderId.includes('/')||reservationId.includes('/')||orderId.includes('\\')||reservationId.includes('\\')||new TextEncoder().encode(orderId).length>1024||new TextEncoder().encode(reservationId).length>1024)return closed(404,'NOT_FOUND','Service order not found.');
 const cookie=request.headers.get('cookie');if(cookie&&new TextEncoder().encode(cookie).length>8192)return unavailable();
 const outgoing=new Headers({host:host!,accept:'application/json'});
 for(const name of ['cookie','origin','x-erpx-context-company','x-erpx-context-revision','x-erpx-context-assignment','x-erpx-context-assignment-version','x-erpx-workspace-branch','x-erpx-workspace-department']){const v=request.headers.get(name);if(v!==null)outgoing.set(name,v);}
 const controller=new AbortController(),cancel=()=>controller.abort();request.signal.addEventListener('abort',cancel,{once:true});if(request.signal.aborted)cancel();const timer=setTimeout(cancel,config.timeoutMs);
 try{
  const url=new URL(`/api/v1/holdings/${encodeURIComponent(slug)}/field-service/${encodeURIComponent(orderId)}/parts-reservations/${encodeURIComponent(reservationId)}`,config.privateBaseUrl);
  const response=await direct(url,outgoing,controller.signal);
  if(response.status===404){void response.body?.cancel();return closed(404,'NOT_FOUND','Service part reservation not found.');}
  if(response.status!==200||response.headers.get('content-type')?.split(';')[0]?.trim()!=='application/json'){void response.body?.cancel();return unavailable();}
  const parsed:unknown=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(await bytes(response.body,2*1024*1024)));
  if(!reservation(parsed,orderId,reservationId))return closed(502,'NATIVE_SERVICE_UNAVAILABLE','Unable to confirm the reservation.');
  return Response.json(parsed,{headers});
 }catch{return unavailable();}finally{clearTimeout(timer);request.signal.removeEventListener('abort',cancel);}
}
