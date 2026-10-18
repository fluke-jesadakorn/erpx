"use client";
import { useRouter as useNextRouter } from 'next/navigation';
import { canonicalUiHref } from '@/lib/work_projection/destination-registry';
import {useProductNavigationScope} from "@/components/workplace/product-link-context";
import {productNavigationHref,isProductHandoffHref,type ProductNavigationScope} from "@/lib/work_projection/product-navigation";
import {hasUnsavedWork} from "@/lib/work_projection/context-switch";
import {useCallback,useMemo} from 'react';
export function destinationHref(href:string,scope:ProductNavigationScope={audience:null}) {
 const url=new URL(href,window.location.href);
 const target=productNavigationHref(href,scope);
 return target.handoff||target.href!==canonicalUiHref(href)?target.href:canonicalUiHref(url.origin===window.location.origin?url.pathname+url.search+url.hash:href);
}
export function useRouter() {
 const router=useNextRouter(),scope=useProductNavigationScope();
 const navigate=useCallback((method:"push"|"replace",href:string,options?:Parameters<typeof router.push>[1])=>{
  const target=destinationHref(href,scope);
  if(scope.audience&&isProductHandoffHref(target)&&hasUnsavedWork()){window.dispatchEvent(new CustomEvent("erpx:product-navigate",{detail:{href:target}}));return;}
  router[method](target,options);
 },[router,scope]);
 return useMemo(()=>({...router,push:(href:string,options?:Parameters<typeof router.push>[1])=>navigate("push",href,options),replace:(href:string,options?:Parameters<typeof router.replace>[1])=>navigate("replace",href,options)}),[router,navigate]);
}
