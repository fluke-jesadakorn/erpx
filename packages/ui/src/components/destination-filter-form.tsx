"use client";
import type {ComponentProps} from 'react';
import {useRouter} from '@erpx/feature-shared/lib/destination-router';
import {canonicalUiHref,workspacePathParts} from '@erpx/feature-work_projection/lib/destination-registry';
/** Native GET filters navigate to the selected server page with ordinary query data. */
export function DestinationFilterForm({onSubmit,action,method,...props}:ComponentProps<'form'>) {
 const router=useRouter();
 return <form {...props} action={action} method={method} onSubmit={event=>{
   onSubmit?.(event);
   if(event.defaultPrevented||method?.toLowerCase()==='post'||typeof action==='function')return;
   const url=new URL(typeof action==='string'?action:window.location.pathname,window.location.href);
   if(url.origin!==window.location.origin||!workspacePathParts(url.pathname))return;
   const query=new URLSearchParams();
   for(const [key,value] of new FormData(event.currentTarget))if(typeof value==='string')query.append(key,value);
   event.preventDefault();
   router.push(canonicalUiHref(url.pathname+(query.size?'?'+query:'')),{scroll:false});
 }}/>
}
