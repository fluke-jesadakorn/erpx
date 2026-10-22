"use client";
import { useEffect, useState } from "react";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import Link from "@erpx/workplace/components/workspace-link";
import { frontWorkReturn } from "@erpx/feature-work_projection/lib/front-work-return";

export function ReturnToFront({ base, holdingSlug, companySlug, companyId, userId, assignmentId }: { base:string; holdingSlug:string; companySlug?:string; companyId:string; userId:string; assignmentId:string }) {
  const search = useSearchParams(), incoming = search.get("frontReturn");
  const [href,setHref] = useState<string|null>(null);
  const key = `erpx-front-return:v1:${base}:${companyId}:${userId}:${assignmentId}`;
  useEffect(()=>{
    const timer = setTimeout(()=>{
      try {
        const current = companySlug ? frontWorkReturn(incoming,holdingSlug,companySlug) : null;
        if(current)sessionStorage.setItem(key,current);
        setHref(current ?? (companySlug ? frontWorkReturn(sessionStorage.getItem(key),holdingSlug,companySlug) : null));
      } catch { setHref(companySlug ? frontWorkReturn(incoming,holdingSlug,companySlug) : null); }
    },0);
    return ()=>clearTimeout(timer);
  },[incoming,holdingSlug,companySlug,key]);
  if(!href)return null;
  return <div className="content" style={{paddingBottom:0,paddingTop:12}} data-app-chrome><Link className="back-link" href={href}>← Return to {href.split("/")[4]==="pos"?"POS":"Hotel PMS"}</Link></div>;
}
