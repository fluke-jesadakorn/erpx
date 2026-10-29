import { routePath, routePrefixes, sourcePath, workspacePathParts, canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {holdingHostForHost,holdingHostForSlug,parseHoldingRoute} from '@erpx/contracts/holding-hosts';
/** Drawer references select an owner reader; they never confer authority. */
import {recordRoutes,recordSections,defaultRecordSection,recordBasePath,type RecordKind,type RecordTarget} from '@erpx/feature-records/lib/record-route-definitions';
export {recordRoutes,recordSections,defaultRecordSection,isRecordKind} from '@erpx/feature-records/lib/record-route-definitions';
export type {RecordKind,RecordTarget} from '@erpx/feature-records/lib/record-route-definitions';
export function recordRouteTemplate(kind:RecordKind,args:Record<string,string>={}){return recordBasePath(kind,args,routePath);}
export function recordTarget(href: string, base: string): RecordTarget | null {
 let url:URL;try{url=new URL(href,'https://erpx.invalid');}catch{return null;}
 if(url.origin!=='https://erpx.invalid'||url.searchParams.get('print')==='1')return null;
 const parts=workspacePathParts(url.pathname,base.match(/^\/holdings\/([^/]+)/)?.[1]);
 const ownerPath=parts?parts.base+'/'+parts.path:url.pathname;
 if(!ownerPath.startsWith(base+'/'))return null;
 const destination=ownerPath.slice(base.length+1),args=Object.fromEntries(url.searchParams);
 // Accept owner-generated inputs during normalization; these are not public aliases.
 const legacy=sourcePath(destination);
 if(legacy==='legal'&&args.record)return {kind:'legal',id:args.record,args};
 if(legacy==='finance/close'&&args.periodId)return {kind:'finance_period',id:args.periodId,args};
 if(legacy==='pos'&&args.sale)return {kind:'pos_sale',id:args.sale,args};
 if(Object.hasOwn(routePrefixes,legacy))return null;
 const order=destination.match(/^(purchasing\/orders|customers-sales\/sales\/documents\/quote)\/([^/]+)(?:\/(overview))?$/);
 if(order)return {kind:'order',id:decodeURIComponent(order[2]),args:{...args,view:order[1]==='purchasing/orders'?'purchase':'quote',section:order[3]??'overview'}};
 const legal=destination.match(/^services\/legal\/(request|matter|contract|obligation|compliance|license|corporate|ip|privacy|counsel|template|rule)\/([^/]+)(?:\/([^/]+))?$/);
 if(legal)return {kind:'legal',id:decodeURIComponent(legal[2]),args:{...args,kind:legal[1],section:legal[3]??'overview'}};
 const flow=destination.match(/^work\/(routing|department-approvals)\/([^/]+)\/([^/]+)(?:\/([^/]+))?$/);
 if(flow)return {kind:'work_flow',id:decodeURIComponent(flow[3]),args:{...args,kind:decodeURIComponent(flow[2]),view:flow[1],section:flow[4]??'overview'}};
 for(const kind of Object.keys(recordRoutes) as RecordKind[]){
  if(['legal','work_flow'].includes(kind))continue;
  const prefix=recordRouteTemplate(kind).split(':id')[0];
  const legacyPrefix=recordRoutes[kind].route.split(':id')[0];
  const rest=destination.startsWith(prefix)?destination.slice(prefix.length):legacy.startsWith(legacyPrefix)?legacy.slice(legacyPrefix.length):null;
  if(!rest)continue;
  const [id,section,...tail]=rest.split('/');
  if(!id||['new','current','archived','settings','history','advances','recoveries','runs','drafts','sources','plans','courses','assignments','mine'].includes(id)||tail.length)continue;
  const selected=section??(kind==='project'?args.view:kind==='person'?args.tab:undefined)??defaultRecordSection(kind);
  if(!(recordSections[kind]??['overview']).includes(selected))continue;
  try{return {kind,id:decodeURIComponent(id),args:{...args,section:selected}};}catch{return null;}
 }
 return null;
}
export function recordCanonicalHref(base:string,target:RecordTarget) {
 const args=target.args??{},section=args.section??(target.kind==='project'?args.view:undefined)??(target.kind==='person'?args.tab:undefined)??defaultRecordSection(target.kind);
 const path=recordRouteTemplate(target.kind,args).replace(':id',encodeURIComponent(target.id))+'/'+encodeURIComponent(section);
 const url=new URL(base+'/'+path,'https://erpx.invalid');
 for(const [key,value] of Object.entries(args))if(!['section','tab','detailKind','detailId','detailArgs','detailPage','detailMode','record','sale','periodId','kind',...(['project','order'].includes(target.kind)?['view']:[]),...(target.kind==='order'?['type']:[])].includes(key))url.searchParams.set(key,value);
 return url.pathname+url.search;
}
export function withoutRecordDrawer(current:string,holdingSlug?:string) {
 const url=new URL(current,'https://erpx.invalid'),holding=holdingSlug??holdingHostForHost(url.host)?.holdingSlug,parts=workspacePathParts(url.pathname,holding),target=parts?recordTarget(url.pathname,parts.base):null;
 const returnTo=url.searchParams.get('returnTo');
 const frontBase=url.pathname.match(/^(\/front\/[^/]+\/[^/]+\/(?:pos|pms)\/(?:holding|branches\/[^/]+(?:\/departments\/[^/]+)?|departments\/[^/]+))\//)?.[1]??url.pathname.match(/^(\/[^/]+\/[^/]+\/(?:holding|branches\/[^/]+(?:\/departments\/[^/]+)?|departments\/[^/]+))\//)?.[1];
 const returnBase=parts?.base??frontBase;
 if(returnTo&&returnBase&&returnTo.startsWith('/')&&!returnTo.startsWith('//')){
  const destination=new URL(returnTo,'https://erpx.invalid');
  // The return path is untrusted query data. Keep it inside the current
  // workspace while preserving list selectors and filters exactly.
  const destinationParts=workspacePathParts(destination.pathname,holding);
  if(destination.origin==='https://erpx.invalid'&&(destination.pathname.startsWith(returnBase+'/')||parts&&destinationParts?.base===parts.base))return destination.pathname+destination.search+destination.hash;
  const from=holding?parseHoldingRoute(holding,url.pathname):null,entry=holding?parseHoldingRoute(holding,destination.pathname):null;
  if(destination.origin==='https://erpx.invalid'&&parts&&holding&&parts.base===`/holdings/${holding}`&&entry?.kind==='entry'&&from?.app===entry.app)return destination.pathname+destination.search+destination.hash;
 }
 if(target){url.pathname=url.pathname.split('/').slice(0,-2).join('/');}
 else url.pathname=url.pathname.replace(/\/records\/[^/]+\/[^/]+\/overview$/,'');
 url.pathname=url.pathname.replace(/\/receipts\/[^/]+\/overview$/,'');
 for(const name of ['returnTo','detailKind','detailId','detailArgs','detailPage','detailMode'])url.searchParams.delete(name);
 return canonicalUiHref(url.pathname+url.search+url.hash);
}
export function recordDrawerHref(current:string,target:RecordTarget,holdingSlug?:string,knownBase?:string) {
 const url=new URL(current,'https://erpx.invalid'),holding=holdingSlug??holdingHostForHost(url.host)?.holdingSlug,parts=workspacePathParts(url.pathname,holding);
 let base=parts?.base;
 if(!base&&holding&&knownBase===`/holdings/${holding}`){
  const host=holdingHostForSlug(holding),entry=parseHoldingRoute(holding,url.pathname),destination=parseHoldingRoute(holding,new URL(recordCanonicalHref(knownBase,target),'https://erpx.invalid').pathname);
  const trustedCurrent=current.startsWith('/')&&!current.startsWith('//')&&url.origin==='https://erpx.invalid'||/^https:\/\//i.test(current)&&!!host&&url.protocol==='https:'&&url.host===host.hostname;
  if(trustedCurrent&&entry?.kind==='entry'&&destination?.app===entry.app)base=knownBase;
 }
 if(!base)return current;
 const next=new URL(recordCanonicalHref(base,target),'https://erpx.invalid');
 next.searchParams.set('returnTo',withoutRecordDrawer(current,holding));
 return next.pathname+next.search;
}
