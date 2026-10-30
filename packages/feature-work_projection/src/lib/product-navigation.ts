import {canonicalUiHref,workspacePathParts,destinations} from '@erpx/feature-work_projection/lib/destination-registry';
import {destinationProduct,developmentProductOrigin,product,productIds,sharedHostKeys,modulePlacement,type ProductHostKey} from '@erpx/contracts/product-registry';
import {safeProductReturnPath} from '@erpx/contracts/account-navigation';
import {adminWorkspaceTarget} from '@erpx/contracts/admin-workspace';
import {accountWorkspaceTarget} from '@erpx/contracts/account-workspace';
import {publicSurfaceTarget,publicSurfaceLaunchPath,legalEnrollmentEntryTarget} from '@erpx/contracts/public-surface-entry';
import type {UrlObject} from 'node:url';
import {holdingHostForHost,holdingOrigin,holdingUiUrl,holdingPublicUiUrl,holdingFrontUiUrl,parseHoldingRoute,contributorExactOrigin,type OriginProfile} from '@erpx/contracts/holding-hosts';
const profileForOrigin=(origin:string|undefined):OriginProfile=>{try{const url=new URL(origin??'');return contributorExactOrigin(url.host,'contributor')===url.origin?'contributor':'public';}catch{return 'public';}};

export type ProductNavigationScope={audience:ProductHostKey|null;companyId?:string;companySlug?:string;workspaceBase?:string;holdingSlug?:string;holdingOrigin?:string};
const credentials=['code','state','token','session_token','_rsc'];
const frontViews={pos:['sale','kitchen','history','shifts','notifications'],pms:['today','front-desk','reservations','rooms','housekeeping','guest-accounts','cashier','night-audit','notifications']};
const legacyFrontOrigins={pos:'https://dev-pos.jesadakorn.com',pms:'https://dev-pms.jesadakorn.com'};
const ownedOrigin=(origin:string)=>[...productIds,...sharedHostKeys].some(key=>developmentProductOrigin(key)===origin)||origin==='https://console-dev-erpx.jesadakorn.com'||Object.values(legacyFrontOrigins).includes(origin);
/** String and relative Next Link URL-object forms share the same owner mapping. */
export function productHrefString(href:string|UrlObject):string|null {
 if(typeof href==='string')return href;
 if(href.protocol||href.host||href.hostname||href.port||href.auth||!href.pathname?.startsWith('/'))return null;
 const query=new URLSearchParams(typeof href.query==='string'?href.query:undefined);
 if(href.query&&typeof href.query==='object')for(const [key,value] of Object.entries(href.query))for(const item of Array.isArray(value)?value:[value])if(item!==undefined)query.append(key,item===null?'':String(item));
 const search=href.search??(query.size?'?'+query:'');
 return href.pathname+(search&&!search.startsWith('?')?'?':'')+search+(href.hash&&!href.hash.startsWith('#')?'#':'')+(href.hash??'');
}
/** A Front launch is a nested GET return, retaining business fragments while
 * the original SSO sanitizer continues to drop top-level callback fragments. */
export function frontProductLaunchTarget(app:'pos'|'pms',value:string|null){
 if(!value||value.length>2000||!value.startsWith('/')||value.startsWith('//')||/[\\\u0000-\u0020\u007f]/.test(value))return null;
 let url:URL;try{url=new URL(value,'https://front.invalid');decodeURIComponent(url.pathname);}catch{return null;}
 if(url.origin!=='https://front.invalid'||/%(?:2f|5c|0[0-9a-f]|1[0-9a-f])/i.test(url.pathname))return null;
 const match=/^\/([A-Za-z0-9_-]+)\/([A-Za-z0-9_-]+)\/(holding|branches\/[A-Za-z0-9_-]+(?:\/departments\/[A-Za-z0-9_-]+)?|departments\/[A-Za-z0-9_-]+)(?:\/([^/]+)(\/.*)?)?\/?$/.exec(url.pathname);
 if(!match||match[4]&&!frontViews[app].includes(match[4]))return null;
 const tail=match[5]??'';
 if(tail&&(app==='pos'?!/^\/receipts\/[^/]+\/overview$/.test(tail):!/^\/records\/(?:pms_record|pms_entry|pms_hotel_sources|pms_hotel_record)\/[^/]+\/overview$/.test(tail)))return null;
 for(const key of credentials)url.searchParams.delete(key);
 return {holdingSlug:match[1],companySlug:match[2],location:match[3],target:url.pathname+url.search+url.hash};
}
function frontFromOffice(app:'pos'|'pms',destination:string,url:URL,scope:ProductNavigationScope){
 if(!scope.companySlug||!scope.workspaceBase)return null;
 const queue=url.searchParams.get('queue');
 if(app==='pms'&&((destination==='pms/reservations'&&queue==='groups')||(destination==='pms/guest-accounts'&&['approval','corrections'].includes(queue??''))||(destination==='pms/cashier'&&queue==='approval')))return null;
 const base=/^\/holdings\/([A-Za-z0-9_-]+)(?:\/(branches\/[A-Za-z0-9_-]+(?:\/departments\/[A-Za-z0-9_-]+)?|departments\/[A-Za-z0-9_-]+))?$/.exec(scope.workspaceBase);
 if(!base)return null;
 let rest=destination.slice(app.length).replace(/^\//,'');
 if(app==='pos'&&/^receipts\//.test(rest))rest='history/'+rest;
 if(app==='pms'&&rest==='dashboard')rest='today';
 const native=`/${base[1]}/${encodeURIComponent(scope.companySlug)}/${base[2]??'holding'}/${rest}${url.search}${url.hash}`;
 return frontProductLaunchTarget(app,native)?.target??null;
}
/** Navigation only. Owner launch, release, current IAM and record policy remain
 * authoritative. No API, callback, mutation or cross-Company scope is rewritten. */
export function productNavigationHref(href:string,scope:ProductNavigationScope):{href:string;handoff:boolean}{
 if(scope.holdingSlug){
  const profile=profileForOrigin(scope.holdingOrigin),origin=holdingOrigin(scope.holdingSlug,profile)!;
  let url:URL;try{url=new URL(href,origin);}catch{return {href,handoff:false};}
  if(!holdingHostForHost(url.host,profile)&&!ownedOrigin(url.origin)||url.username||url.password)return {href,handoff:false};
  const frontApp=url.pathname.match(/^\/front\/[^/]+\/[^/]+\/(pos|pms)(?:\/|$)/)?.[1] as 'pos'|'pms'|undefined;
  const mapped=holdingUiUrl(canonicalUiHref(url.pathname+url.search+url.hash),scope.companySlug??scope.companyId,profile)
    ??holdingPublicUiUrl(url.pathname+url.search+url.hash,profile)
    ??(frontApp||scope.audience==='pos'||scope.audience==='pms'?holdingFrontUiUrl(url.pathname+url.search+url.hash,frontApp??scope.audience as 'pos'|'pms',profile):null);
  if(mapped){const destination=new URL(mapped),holding=holdingHostForHost(destination.host,profile),owner=holding&&parseHoldingRoute(holding.holdingSlug,destination.pathname)?.app;return {href:mapped,handoff:destination.origin!==origin||owner!==scope.audience};}
  const holding=holdingHostForHost(url.host,profile);
  if(holding)return {href:url.href,handoff:url.origin!==origin||parseHoldingRoute(holding.holdingSlug,url.pathname)?.app!==scope.audience};
 }
 const raw={href,handoff:false},unchanged={href:canonicalUiHref(href),handoff:false};
 if(href.startsWith('//'))return raw;
 if(!scope.audience||href.startsWith('#')||!href.startsWith('/')&&!/^https:\/\//.test(href)||href.startsWith('//'))return unchanged;
 const origin=developmentProductOrigin(scope.audience);let url:URL;
 try{url=new URL(href,origin);}catch{return unchanged;}
 if(url.username||url.password||!ownedOrigin(url.origin))return raw;
 if(/^\/(?:api|sso|login|launch|choose-work-role)(?:\/|$)/.test(url.pathname))return raw;
 const account=accountWorkspaceTarget(href.startsWith('/')?href:/^https:\/\/[^/]+(\/.*)$/.exec(href)?.[1]);
 if(account){
  if(scope.audience==='account')return {href:account,handoff:false};
  const next=safeProductReturnPath('/account/launch?'+new URLSearchParams({next:account}));
  if(next==='/')return unchanged;
  return {href:new URL(next,developmentProductOrigin('account')).href,handoff:true};
 }
 const rawPublicPath=href.startsWith('/')?href:/^https:\/\/[^/]+(\/.*)$/.exec(href)?.[1],enrollment=legalEnrollmentEntryTarget(rawPublicPath);
 if(enrollment)return scope.audience==='account'?{href:enrollment,handoff:false}:{href:new URL(enrollment,developmentProductOrigin('account')).href,handoff:true};
 const external=publicSurfaceTarget(rawPublicPath);
 if(external){
  if(external.host===scope.audience)return {href:external.target,handoff:false};
  if(external.host==='booking'||external.host==='forms')return {href:new URL(external.target,developmentProductOrigin(external.host)).href,handoff:true};
  const next=publicSurfaceLaunchPath(external.target);if(!next)return unchanged;
  const destination=new URL('/sso/start',developmentProductOrigin(external.host));destination.searchParams.set('next',next);return {href:destination.href,handoff:true};
 }
 if(!scope.companyId||!scope.workspaceBase)return unchanged;
 // Corrections share the Guest Accounts collection with the registered
 // approval alias. Canonical selection consumes its queue key, so retain this
 // actual owner review mode before deciding between daily and backoffice.
 const original=workspacePathParts(url.pathname),reviewQueue=original?.path==='pms/guest-accounts'&&url.searchParams.get('queue')==='corrections'?'corrections':null;
 url=new URL(canonicalUiHref(url.href));
 if(reviewQueue)url.searchParams.set('queue',reviewQueue);
 const parts=workspacePathParts(url.pathname);let owner:ProductHostKey|undefined,target:string|null=null;
 if(parts){
  if(parts.base!==scope.workspaceBase)return unchanged;
  const admin=adminWorkspaceTarget(scope.companyId,url.pathname+url.search+url.hash);
  if(admin&&destinations.some(item=>(item.path==='erpx-management'||item.path.startsWith('erpx-management/')||item.path.startsWith('administration/'))&&(parts.path===item.path||parts.path.startsWith(item.path+'/')||item.path.startsWith(parts.path+'/')))){
   if(['companyId','company','company_id'].some(key=>url.searchParams.getAll(key).some(value=>value!==scope.companyId)))return unchanged;
   owner=modulePlacement.identity_access;
  }
  owner??=parts.path==='pos'||parts.path.startsWith('pos/')?'pos':parts.path==='pms'||parts.path.startsWith('pms/')?'pms':destinationProduct(parts.path);
  if(!owner)return unchanged;
  const backofficeRoot=(owner==='pos'||owner==='pms')&&scope.audience===owner&&parts.path===owner&&!url.searchParams.has('tab');
  target=(owner==='pos'||owner==='pms')&&!backofficeRoot?(frontFromOffice(owner,parts.path,url,scope)??url.pathname+url.search+url.hash):url.pathname+url.search+url.hash;
 }else{
  const alias=/^\/front\/([^/]+)\/([^/]+)\/(pos|pms)\/(.*)$/.exec(url.pathname);
  owner=alias?.[3] as 'pos'|'pms'|undefined;
  if(alias){url.pathname=`/${alias[1]}/${alias[2]}/${alias[4]}`;}
  if(!owner)owner=(['pos','pms'] as const).find(app=>(url.origin===developmentProductOrigin(app)||url.origin===legacyFrontOrigins[app]))??(scope.audience==='pos'||scope.audience==='pms'?scope.audience:undefined);
  if(owner!=='pos'&&owner!=='pms')return unchanged;
  const front=frontProductLaunchTarget(owner,url.pathname+url.search+url.hash);
  if(!front||front.companySlug!==scope.companySlug)return unchanged;
  const workspace=`/holdings/${front.holdingSlug}${front.location==='holding'?'':'/'+front.location}`;
  if(workspace!==scope.workspaceBase)return unchanged;
  target=front.target;
 }
 if(!target)return unchanged;
 if(owner===scope.audience)return {href:target,handoff:false};
 const clean=new URL(target,'https://target.invalid');for(const key of credentials)clean.searchParams.delete(key);
 const retained=clean.pathname+clean.search+clean.hash;
 if(owner!=='admin'&&product(owner)?.shell!=='office'&&owner!=='pos'&&owner!=='pms')return unchanged;
 const next=safeProductReturnPath((owner==='admin'?'/admin/launch?':'/launch?')+new URLSearchParams({companyId:scope.companyId,next:retained}));
 if(owner==='admin'&&next==='/')return unchanged;
 const result=new URL('/sso/start',developmentProductOrigin(owner));result.searchParams.set('next',next);
 return {href:result.href,handoff:true};
}

/** Only the already configured one-level development SSO destinations can be
 * requested by an imperative dirty-guard event. This is never an access grant. */
export function isProductHandoffHref(value:string){
 try{const url=new URL(value);if(url.username||url.password)return false;
  if(holdingHostForHost(url.host,profileForOrigin(url.origin))&&url.protocol==='https:')return !/^\/(?:__erpx|api)(?:\/|$)/.test(url.pathname);
  if(url.origin===developmentProductOrigin('account')&&url.pathname==='/account/launch')return !url.hash&&url.searchParams.getAll('next').length===1&&[...url.searchParams.keys()].every(key=>key==='next')&&accountWorkspaceTarget(url.searchParams.get('next'))!==null;
  if(url.origin===developmentProductOrigin('account')&&legalEnrollmentEntryTarget(url.pathname+url.search+url.hash))return true;
  const publicTarget=publicSurfaceTarget(url.pathname+url.search+url.hash);
  if(publicTarget&&(publicTarget.host==='booking'||publicTarget.host==='forms')&&url.origin===developmentProductOrigin(publicTarget.host))return true;
  return url.pathname==="/sso/start"&&[...productIds,...sharedHostKeys].some(key=>url.origin===developmentProductOrigin(key));}catch{return false;}
}
