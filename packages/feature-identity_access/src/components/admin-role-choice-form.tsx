'use client';
import {useRef,useState,type ReactNode} from 'react';
import {confirmContextSwitch,contextIsSwitching,setContextSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {useHydrated} from '@erpx/ui/components/use-hydrated';

export async function submitAdminRoleChoice(form:HTMLFormElement){
 if(contextIsSwitching()||!confirmContextSwitch())return false;
 const body=new URLSearchParams();
 for(const[key,value]of new FormData(form)){
  if(typeof value!=='string')throw Error('Submit the displayed work role form.');
  body.append(key,value);
 }
 setContextSwitching(true);
 try{
  // Native navigation POSTs carry Origin:null under no-referrer. CORS-mode
  // fetch preserves the real Origin without relaxing the server's strict guard.
  const response=await fetch('/admin/launch',{method:'POST',mode:'cors',credentials:'same-origin',referrerPolicy:'no-referrer',body});
  if(!response.ok)throw Error('The work role could not be selected. Check the current choices and try again.');
  const destination=new URL(response.url);
  if(!response.redirected||destination.origin!==window.location.origin)throw Error('The work role response could not be verified. Reload the current choices.');
  window.location.assign(destination.href);
  return true;
 }catch(error){setContextSwitching(false);throw error;}
}

/** The original form endpoint repeats session, Company and assignment CAS. */
export function AdminRoleChoiceForm({children}:{children:ReactNode}){
 const ready=useHydrated(),pending=useRef(false),[busy,setBusy]=useState(false),[error,setError]=useState('');
 return <form method="post" action="/admin/launch" onSubmit={async event=>{
  event.preventDefault();if(pending.current||!ready)return;
  pending.current=true;setBusy(true);setError('');
  try{if(await submitAdminRoleChoice(event.currentTarget))return;}
  catch(cause){setError(cause instanceof Error?cause.message:'The work role could not be selected. Try again.');}
  pending.current=false;setBusy(false);
 }}><fieldset disabled={!ready||busy} style={{border:0,padding:0,margin:0,minWidth:0}}>{children}</fieldset>{error&&<p role="alert">{error}</p>}</form>;
}
