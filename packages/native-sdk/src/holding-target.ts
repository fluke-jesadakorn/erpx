import 'server-only';
import {contributorExactOrigin,type OriginProfile} from '@erpx/contracts/holding-hosts';
export type NativeHoldingTarget={origin:string;holdingId:string;holdingSlug:string};
export type NativeHoldingConfiguration={privateBaseUrl:string;timeoutMs:number;holdings:NativeHoldingTarget[]};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const fail=():never=>{throw Error('NATIVE_HOLDING_CONFIGURATION_INVALID');};
/** Shared bounded private transport configuration. It supplies no account,
 * Company or permission; native Identity still verifies each forwarded cookie. */
export function nativeHoldingConfiguration(raw:string,profile:OriginProfile='public'):NativeHoldingConfiguration{
 let c:unknown;try{c=JSON.parse(raw);}catch{return fail();}
 if(!object(c)||typeof c.privateBaseUrl!=='string'||typeof c.timeoutMs!=='number'||!Number.isSafeInteger(c.timeoutMs)||c.timeoutMs<250||c.timeoutMs>30000||!Array.isArray(c.holdings)||!c.holdings.length||c.holdings.length>64)return fail();
 let base:URL;try{base=new URL(c.privateBaseUrl);}catch{return fail();}const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||[3000,3001,3100,3101,3103,5432,5433,5435,8080].includes(port)||port>=3200&&port<=3299||base.username||base.password||base.origin!==c.privateBaseUrl)return fail();
 const hosts=new Set<string>(),slugs=new Set<string>();
 for(const h of c.holdings){
  if(!object(h)||typeof h.origin!=='string'||typeof h.holdingId!=='string'||!h.holdingId||new TextEncoder().encode(h.holdingId).length>200||typeof h.holdingSlug!=='string'||!/^[a-z0-9_-]{1,128}$/.test(h.holdingSlug))return fail();
  let u:URL;try{u=new URL(h.origin);}catch{return fail();}
  if(u.protocol!=='https:'||u.origin!==h.origin||u.username||u.password||u.port&&contributorExactOrigin(u.host,profile)!==h.origin||!u.hostname.includes('.')||!u.hostname.split('.').every(p=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p))||hosts.has(u.host)||slugs.has(h.holdingSlug))return fail();hosts.add(u.host);slugs.add(h.holdingSlug);
 }
 return c as NativeHoldingConfiguration;
}
