import 'server-only';
import {portalContext} from '@erpx/server-legal/adapters/legal/portal';
import {legalPortalPurchasePredicate} from '@erpx/server-legal/adapters/portal-purchases';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {configuredProductOrigin,configuredProductHandoff,productHostForHost} from '@erpx/feature-runtime/lib/product-hosts';
import {legalEnrollmentReturnPath,publicSurfaceLaunchPath} from '@erpx/server-identity_access/public/rules/public-surface-entry';
/** Rechecks the actual verified central account against the retained original
 * share. This navigation neither accepts an invitation nor records consent. */
export async function legalEnrollmentContinuation(holdingSlug:string,shareId:string){
 const {write,actor}=await portalContext(holdingSlug);
 const source=await write.prepare(`SELECT s.id FROM legal_shares s JOIN legal_items i ON i.id=s.item_id JOIN legal_documents d ON d.id=s.document_id AND d.item_id=i.id JOIN legal_settings settings ON settings.company_id=i.company_id AND settings.enabled WHERE s.id=? AND lower(s.email)=? AND (s.user_id IS NULL OR s.user_id=?) AND s.revoked_at IS NULL AND s.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND ${await legalPortalPurchasePredicate('i')} AND NOT EXISTS(SELECT 1 FROM legal_destructions destruction WHERE destruction.item_id=i.id AND destruction.state IN ('executing','completed'))`).bind(shareId,actor.email,actor.userId).first<{id:string}>();
 if(!source)throw new ApiError(404,'LEGAL_SHARE_UNAVAILABLE','This share expired or is unavailable.');
 return `/legal-portal/${encodeURIComponent(holdingSlug)}?`+new URLSearchParams({share:source.id});
}
const headers={'Cache-Control':'private, no-store','Referrer-Policy':'no-referrer','X-Content-Type-Options':'nosniff'};
export async function legalEnrollmentContinueRequest(request:Request){
 const origin=configuredProductOrigin('account'),url=new URL(request.url);
 if(!origin||productHostForHost(request.headers.get('host'))!=='account'||url.origin!==origin||request.headers.get('host')!==url.host||url.pathname!=='/legal-portal/continue')return new Response('Not found',{status:404,headers});
 if(request.method!=='GET')return new Response('Method not allowed',{status:405,headers:{...headers,Allow:'GET'}});
 const continuation=legalEnrollmentReturnPath(url.pathname+url.search),holding=url.searchParams.get('holding'),share=url.searchParams.get('share');if(!continuation||!holding||!share)return new Response('Invalid invitation return',{status:422,headers});
 try{
  const target=await legalEnrollmentContinuation(holding,share),next=publicSurfaceLaunchPath(target),destination=next&&configuredProductHandoff('legal-portal',next);
  if(!destination)return new Response('Not found',{status:404,headers});
  return new Response(null,{status:303,headers:{...headers,Location:destination}});
 }catch(error){
  if(error instanceof Error&&error.message==='UNAUTHENTICATED'||error instanceof ApiError&&error.status===401){const login=new URL('/login',origin);login.searchParams.set('next',continuation);return new Response(null,{status:303,headers:{...headers,Location:login.href}});}
  const response=errorResponse(error);for(const [key,value] of Object.entries(headers))response.headers.set(key,value);return response;
 }
}
