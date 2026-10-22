"use client";
import Link from "@erpx/workplace/components/workspace-link";
import {usePathname} from 'next/navigation';
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { Button } from "@erpx/ui/components/button";
import { RecordDrawer } from "@erpx/feature-records/components/record-drawer";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";

export { CollectionRecordLink } from "@erpx/feature-records/components/collection-record-link";

/** The owning API returns its authorized snapshot; presentation never widens scope. */
export function RecordCollectionDrawer<T>({kind,title,endpoint,select,render,recovery}: {kind:string;title:string;endpoint:string;recovery?:ReactNode;select:(snapshot:T,id:string)=>Record<string,unknown>|null;render?:(record:Record<string,unknown>,snapshot:T)=>ReactNode}) {
 const pathname=usePathname();
 const query=useSearchParams(),id=query.get('detailKind')===kind?query.get('detailId'):null;
 const [loaded,setLoaded]=useState<{id:string;endpoint:string;record:Record<string,unknown>;snapshot:T}|null>(null),[error,setError]=useState<{id:string;endpoint:string;message:string}|null>(null),[retry,setRetry]=useState(0);
 const selector=useRef(select);
 useEffect(()=>{selector.current=select;},[select]);
 useEffect(()=>{
  if(!id)return;
  const controller=new AbortController();let generation=0;
  const load=async()=>{
   const reading=++generation;
   try {const response=await workFetch(endpoint,{cache:'no-store',signal:controller.signal});const body=await response.json();
    if(!response.ok)throw new Error(body.error?.message??'Unable to read this record.');
    const snapshot=body.data as T,record=selector.current(snapshot,id);
    if(!record)throw new Error('Record not found in this work scope.');
    if(!controller.signal.aborted&&reading===generation){setLoaded({id,endpoint,record,snapshot});setError(null);}
   }catch(cause){if(!controller.signal.aborted&&reading===generation)setError({id,endpoint,message:cause instanceof Error?cause.message:'Unable to read this record.'});}
  };
  void load();window.addEventListener('erpx:work-changed',load);
  return ()=>{controller.abort();window.removeEventListener('erpx:work-changed',load);};
 },[id,endpoint,retry]);
 if(!id)return null;
 const full=new URLSearchParams(query.toString());full.set('detailMode','page');
 const currentError=error?.id===id&&error.endpoint===endpoint?error.message:null;
 const content=currentError?<div role="alert"><p>{currentError}</p><Button appearance="standard" onClick={()=>{setError(null);setRetry(retry+1);}}>Retry</Button></div>:loaded?.id===id&&loaded.endpoint===endpoint?<div className="ui-stack"><h1>{title}</h1>{render?render(loaded.record,loaded.snapshot):<RecordFacts record={loaded.record}/>}</div>:<p role="status">Loading record details…</p>;
 return <RecordDrawer key={`${kind}:${id}`} title={title} fullHref={`${pathname}?${full}`}>{recovery?<>{recovery}{content}</>:content}</RecordDrawer>;
}
/** Deliberate field selection avoids exposing credentials or raw internal payloads. */
export function RecordFacts({record,fields}: {record:Record<string,unknown>;fields?:string[]}) {
 const names=fields??['id','name','displayName','description','note','reason','state','status','version','role','scope_kind','scope_node_id','source','user_id','person_id','valid_until','active','connection_id','external_id','observed_at','slug','holdingId','requestedBy','requestedAt','activatedAt','error','companyName','branchName','type','quantity','unit','period','pricingStatus','estimatedSatang','category','provider','service_start','service_end','amount_satang','invoice_id','simulated'];
 return <dl className="ui-form-grid">{names.filter(key=>record[key]!==undefined&&record[key]!==null).map(key=><div key={key}><dt className="muted">{key.replaceAll('_',' ').replace(/([a-z])([A-Z])/g,'$1 $2')}</dt><dd>{['valid_until','observed_at','requestedAt','activatedAt'].includes(key)&&(typeof record[key]==='number'||typeof record[key]==='string')?<TimeText appearance="standard" value={record[key] as number|string}/>:String(record[key])}</dd></div>)}</dl>;
}
