"use client";
import { useRouter as useNextRouter } from 'next/navigation';
import { canonicalUiHref } from '@erpx/feature-work_projection/lib/destination-registry';
import {useProductNavigationScope} from "@erpx/workplace/components/product-link-context";
import {productNavigationHref,type ProductNavigationScope} from "@erpx/feature-work_projection/lib/product-navigation";
import {hasUnsavedWork} from "@erpx/feature-work_projection/lib/context-switch";
import {useCallback,useMemo} from 'react';
export function destinationHref(href:string,scope:ProductNavigationScope={audience:null},options:{preserveQuery?:boolean}={}) {
 const url=new URL(href,window.location.href);
 const target=productNavigationHref(href,scope);
 if(options.preserveQuery&&!target.handoff){
  const normalized=new URL(target.href,window.location.href);
  if(normalized.origin===url.origin&&normalized.pathname===url.pathname){normalized.search=url.search;normalized.hash=url.hash;return normalized.pathname+normalized.search+normalized.hash;}
 }
 return target.handoff||target.href!==canonicalUiHref(href)?target.href:canonicalUiHref(url.origin===window.location.origin?url.pathname+url.search+url.hash:href);
}
export function useRouter() {
 const router=useNextRouter(),scope=useProductNavigationScope();
 const navigate=useCallback((method:"push"|"replace",href:string,options?:Parameters<typeof router.push>[1],preserveQuery=false)=>{
  const target=destinationHref(href,scope,{preserveQuery});
  const handoff=productNavigationHref(href,scope).handoff;
  if(scope.audience&&handoff&&hasUnsavedWork()){window.dispatchEvent(new CustomEvent("erpx:product-navigate",{detail:{href:target}}));return;}
  if(scope.holdingSlug&&handoff){window.location[method==='replace'?'replace':'assign'](target);return;}
  router[method](target,options);
 },[router,scope]);
 return useMemo(()=>({...router,push:(href:string,options?:Parameters<typeof router.push>[1])=>navigate("push",href,options),replace:(href:string,options?:Parameters<typeof router.replace>[1])=>navigate("replace",href,options),pushPreservingQuery:(href:string,options?:Parameters<typeof router.push>[1])=>navigate("push",href,options,true)}),[router,navigate]);
}
