import 'server-only';
import {configuredProductOrigin,configuredProductHandoff,productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {publicSurfaceTarget,type PublicSurfaceHost} from '@erpx/server-identity_access/domain/public-surface-entry';
import {currentSession} from '@erpx/server-identity_access/adapters/authorization';
import {ApiError} from '@erpx/server-runtime/public/http';
export function publicSurfaceOrigin(host:PublicSurfaceHost|'account',legacy:string):string;
export function publicSurfaceOrigin(host:PublicSurfaceHost|'account',legacy:string|undefined):string|undefined;
export function publicSurfaceOrigin(host:PublicSurfaceHost|'account',legacy:string|undefined){return configuredProductOrigin(host)??legacy;}
const headers={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff'};
export async function publicSurfaceLaunchRequest(request:Request){
 const host=productHostForHost(request.headers.get('host')),url=new URL(request.url);
 if(host!=='portal'&&host!=='legal-portal')return new Response('Not found',{status:404,headers});
 const origin=configuredProductOrigin(host);
 if(!origin||url.origin!==origin||request.headers.get('host')!==url.host||url.pathname!=='/public/launch')return new Response('Not found',{status:404,headers});
 if(request.method!=='GET')return new Response('Method not allowed',{status:405,headers:{...headers,Allow:'GET'}});
 const target=url.searchParams.getAll('next').length===1&&[...url.searchParams.keys()].every(key=>key==='next')?publicSurfaceTarget(url.searchParams.get('next')):null;
 if(!target||target.host!==host)return new Response('Invalid public destination',{status:422,headers});
 try{await currentSession();}catch(error){
  if(!(error instanceof Error&&error.message==='UNAUTHENTICATED')&&!(error instanceof ApiError&&error.status===401))throw error;
  const next='/public/launch?'+new URLSearchParams({next:target.target}),destination=configuredProductHandoff(host,next);
  if(!destination)return new Response('Not found',{status:404,headers});
  return new Response(null,{status:303,headers:{...headers,Location:destination}});
 }
 return new Response(null,{status:303,headers:{...headers,Location:new URL(target.target,origin).href}});
}
