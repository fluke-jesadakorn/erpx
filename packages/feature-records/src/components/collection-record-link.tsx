"use client";
import Link from "@erpx/workplace/components/workspace-link";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import type { ReactNode } from "react";

export function CollectionRecordLink({kind,id,children,args}: {kind:string;id:string;children:ReactNode;args?:Record<string,string>}) {
 const pathname=usePathname(),query=useSearchParams(),q=new URLSearchParams(query.toString());
 const collection=pathname.replace(/\/records\/[^/]+\/[^/]+\/overview$/,'');
 for(const key of ['detailKind','detailId','detailMode','detailArgs','detailPage'])q.delete(key);
 for(const [key,value] of Object.entries(args??{}))q.set(key,value);
 const returnQuery=new URLSearchParams(q);returnQuery.delete('returnTo');
 q.set('returnTo',`${collection}${returnQuery.size?'?'+returnQuery:''}`);
 const href=`${collection}/records/${encodeURIComponent(kind)}/${encodeURIComponent(id)}/overview?${q}`;
 return <Link href={href} scroll={false} data-record-link="true" onClick={event=>{
  if(event.button!==0||event.metaKey||event.ctrlKey||event.shiftKey||event.altKey)return;
  if(window.getSelection()?.toString()){event.preventDefault();return;}
  event.currentTarget.setAttribute('data-record-trigger','true');
  if(document.querySelector('[data-record-drawer-content][data-record-dirty="true"]')){event.preventDefault();window.dispatchEvent(new CustomEvent('erpx:record-navigate',{detail:{href}}));}
 }}>{children}</Link>;
}
