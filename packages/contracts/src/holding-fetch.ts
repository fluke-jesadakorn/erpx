import {holdingHostForHost,holdingTransportPath} from './holding-hosts.ts';
import {isProductHostKey} from './product-registry.ts';

/** Client transport selects an app process; it cannot supply owner authority. */
export function holdingFetch(input:RequestInfo|URL,init?:RequestInit):Promise<Response>{
  if(typeof window==='undefined'||!holdingHostForHost(window.location.host))return globalThis.fetch(input,init);
  const app=document.documentElement.dataset.erpxApp;
  if(!app||!isProductHostKey(app))return globalThis.fetch(input,init);
  const value=typeof input==='string'?input:input instanceof URL?input.href:input.url;
  const url=new URL(value,window.location.href);
  if(url.origin!==window.location.origin||!url.pathname.startsWith('/api/'))return globalThis.fetch(input,init);
  url.pathname=holdingTransportPath(app,url.pathname);
  return globalThis.fetch(input instanceof Request?new Request(url,input):url,init);
}
