import {routeContextHref,type RouteSearch} from '@/lib/shared/route-context';
import {canonicalUiHref} from '@/lib/work_projection/destination-registry';

export function handbookCreateHref(base:string,section:string,query:RouteSearch){
 const returnTo=routeContextHref(`${base}/knowledge/handbook/${section}`,query,{returnTo:null,detailKind:null,detailId:null,detailArgs:null,detailPage:null,detailMode:null});
 return routeContextHref(`${base}/handbook/new`,{returnTo});
}

/** A return hint selects only an existing Handbook view in this workspace. */
export function handbookCreateReturn(base:string,returnTo?:string){
 const fallback=canonicalUiHref(`${base}/handbook`);
 if(!returnTo)return fallback;
 try{
  const url=new URL(canonicalUiHref(returnTo),'https://handbook.invalid');
  if(url.origin!=='https://handbook.invalid'||!['published','drafts','editors'].some(section=>url.pathname===`${base}/knowledge/handbook/${section}`))return fallback;
  url.searchParams.delete('returnTo');
  return url.pathname+url.search+url.hash;
 }catch{return fallback;}
}
