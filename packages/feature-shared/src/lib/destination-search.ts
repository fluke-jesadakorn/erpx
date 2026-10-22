"use client";
import { usePathname, useSearchParams as useNextSearchParams } from 'next/navigation';
import { selectionForPath, workspacePathParts } from '@erpx/feature-work_projection/lib/destination-registry';
import { recordTarget } from '@erpx/feature-records/lib/record-drawer';
import { useMemo } from 'react';
/** Controls receive their route selection along with ordinary query filters. */
export function useSearchParams() {
 const pathname=usePathname(),query=useNextSearchParams();
 return useMemo(()=>{const parts=workspacePathParts(pathname),target=parts?recordTarget(pathname,parts.base):null;
 const detail=target?{...target.args,detailKind:target.kind,detailId:target.id,...(target.kind==='project'?{view:target.args?.section??'overview'}:{}),...(target.kind==='person'?{tab:target.args?.section??'profile'}:{})}:{};
 const merged=new URLSearchParams(query);for(const [key,value] of Object.entries({...selectionForPath(pathname),...detail}))if(typeof value==='string')merged.set(key,value);return merged;},[pathname,query]);
}
