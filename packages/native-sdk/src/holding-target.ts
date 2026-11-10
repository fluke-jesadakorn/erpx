import 'server-only';
import {contributorExactOrigin,type OriginProfile} from '@erpx/contracts/holding-hosts';
import {allowedPrivateLoopbackPort} from './loopback';
export type NativeHoldingTarget={origin:string;holdingId:string;holdingSlug:string};
export type NativeProductTarget={origin:string;holdingId:string};
export type NativeHoldingConfiguration={privateBaseUrl:string;timeoutMs:number;holdings:NativeHoldingTarget[];products?:NativeProductTarget[];production?:boolean};
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const fail=():never=>{throw Error('NATIVE_HOLDING_CONFIGURATION_INVALID');};
/** Shared bounded private transport configuration. It supplies no account,
 * Company or permission; native Identity still verifies each forwarded cookie. */
export function nativeHoldingConfiguration(raw:string,profile:OriginProfile='public'):NativeHoldingConfiguration{
 let c:unknown;try{c=JSON.parse(raw);}catch{return fail();}
 if(!object(c)||(c.production!==undefined&&typeof c.production!=='boolean')||typeof c.privateBaseUrl!=='string'||typeof c.timeoutMs!=='number'||!Number.isSafeInteger(c.timeoutMs)||c.timeoutMs<250||c.timeoutMs>30000||!Array.isArray(c.holdings)||!c.holdings.length||c.holdings.length>64)return fail();
 let base:URL;try{base=new URL(c.privateBaseUrl);}catch{return fail();}const port=Number(base.port);
 if(base.protocol!=='http:'||!['127.0.0.1','[::1]'].includes(base.hostname)||!base.port||!allowedPrivateLoopbackPort(port,c.production===true)||base.username||base.password||base.origin!==c.privateBaseUrl)return fail();
 const hosts=new Set<string>(),slugs=new Set<string>(),ids=new Set<string>();
 for(const h of c.holdings){
  if(!object(h)||typeof h.origin!=='string'||typeof h.holdingId!=='string'||!h.holdingId||new TextEncoder().encode(h.holdingId).length>200||typeof h.holdingSlug!=='string'||!/^[a-z0-9_-]{1,128}$/.test(h.holdingSlug))return fail();
  let u:URL;try{u=new URL(h.origin);}catch{return fail();}
  if(profile==='loopback'){
   const port=Number(u.port);
   if(c.production===true||u.protocol!=='http:'||!u.hostname.endsWith('.erpxbrowser.test')||!u.port||!allowedPrivateLoopbackPort(port,false)||u.username||u.password||u.origin!==h.origin||hosts.has(u.host)||slugs.has(h.holdingSlug))return fail();
  }else if(u.protocol!=='https:'||u.origin!==h.origin||u.username||u.password||u.port&&contributorExactOrigin(u.host,profile)!==h.origin||!u.hostname.includes('.')||!u.hostname.split('.').every(p=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p))||hosts.has(u.host)||slugs.has(h.holdingSlug))return fail();
  hosts.add(u.host);slugs.add(h.holdingSlug);ids.add(h.holdingId);
 }
 if(c.products!==undefined){
  if(!Array.isArray(c.products)||c.products.length>64)return fail();
  for(const product of c.products){
   if(!object(product)||typeof product.origin!=='string'||typeof product.holdingId!=='string'||!ids.has(product.holdingId))return fail();
   let u:URL;try{u=new URL(product.origin);}catch{return fail();}
   if(profile==='loopback'){
    const port=Number(u.port);
    if(c.production===true||u.protocol!=='http:'||!u.hostname.endsWith('.erpxbrowser.test')||!u.port||!allowedPrivateLoopbackPort(port,false)||u.username||u.password||u.origin!==product.origin||hosts.has(u.host))return fail();
   }else if(u.protocol!=='https:'||u.origin!==product.origin||u.username||u.password||u.port&&contributorExactOrigin(u.host,profile)!==product.origin||!u.hostname.includes('.')||!u.hostname.split('.').every(p=>/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(p))||hosts.has(u.host))return fail();
   hosts.add(u.host);
  }
 }
 return c as NativeHoldingConfiguration;
}
/** Exact registered host selection. A product origin aliases one Holding; the
 * original product Host/Origin stay on the wire so its SSO audience survives. */
export function nativeHoldingTargetForHost(config:NativeHoldingConfiguration,host:string|null|undefined):NativeHoldingTarget|undefined{
 if(!host)return undefined;
 const direct=config.holdings.find(target=>new URL(target.origin).host===host);
 if(direct)return direct;
 const product=config.products?.find(target=>new URL(target.origin).host===host);
 return product?config.holdings.find(target=>target.holdingId===product.holdingId):undefined;
}
export function nativeHoldingTargetById(config:NativeHoldingConfiguration,holdingId:string):NativeHoldingTarget|undefined{
 return config.holdings.find(target=>target.holdingId===holdingId);
}
