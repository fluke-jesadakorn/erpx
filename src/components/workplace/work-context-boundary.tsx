'use client';
import {useEffect,useRef,useState} from 'react';
export function WorkContextBoundary({base,companyId,assignmentId,revision,frontApp,frontLocation,children}:{base:string;companyId:string;assignmentId:string;revision:number;frontApp?:"pos"|"pms";frontLocation?:string;children:React.ReactNode}){
 const [stale,setStale]=useState(false),root=useRef<HTMLDivElement>(null);
 useEffect(()=>{
 const channel=new BroadcastChannel('erpx-work-context');
 async function check(){try{const r=await fetch(`/api/v1${base}/assignments/active`,{cache:'no-store',headers:{'x-erpx-context-company':companyId}});if(!r.ok){setStale(true);return;}const {data}=await r.json();if(!data.valid||data.revision!==revision||data.assignmentId!==assignmentId||data.companyId!==companyId)setStale(true);}catch{/* Revalidation is retried on focus; writes still check server context. */}}
 channel.onmessage=check;window.addEventListener('focus',check);window.addEventListener('erpx:work-context-changed',check);
 function dirty(){if(root.current)root.current.dataset.workDirty='true';}
 const node=root.current;node?.addEventListener('input',dirty);
 return()=>{channel.close();window.removeEventListener('focus',check);window.removeEventListener('erpx:work-context-changed',check);node?.removeEventListener('input',dirty);};
 },[base,companyId,assignmentId,revision]);
 return <div ref={root} data-work-company={companyId} data-work-assignment={assignmentId} data-work-revision={revision} data-work-stale={String(stale)} data-front-app={frontApp} data-front-location={frontLocation}>
 {stale&&<div role="status" style={{padding:'8px 20px',fontSize:12,borderBottom:'1px solid var(--line)',color:'var(--muted)'}}>Work role changed. Your open form has not been submitted. Reload and review before continuing. <button onClick={()=>{if(!root.current?.dataset.workDirty||window.confirm('Reload and discard unsaved changes?'))window.location.reload();}}>Reload</button></div>}
 <div hidden={stale} inert={stale}>{children}</div></div>;
}
