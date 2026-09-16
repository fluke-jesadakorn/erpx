'use client';
import {useHydrated} from '@/components/ui/use-hydrated';
import {useState} from 'react';
import {RoleSelect} from './role-select';
import type {WorkAssignment} from '@/server/authorization';
import styles from './account.module.css';
export function WorkRolePicker({base,options,revision}:{base:string;options:(WorkAssignment&{href:string})[];revision:number}){
 const ready=useHydrated();
 const [pending,setPending]=useState(false),[error,setError]=useState('');
 return <div className={styles.skin}><RoleSelect assignments={options} value="" disabled={!ready || pending} onChange={async assignmentId=>{
 setPending(true);setError('');try{
 const r=await fetch(`/api/v1${base}/assignments/active`,{method:'POST',headers:{'content-type':'application/json','x-erpx-context-company':options.find(option=>option.id===assignmentId)?.companyId??''},body:JSON.stringify({assignmentId,revision})});
 const body=await r.json();if(!r.ok)throw Error(body.error?.message??'Role could not be selected.');
 const channel=new BroadcastChannel('erpx-work-context');channel.postMessage({changed:true});channel.close();window.location.assign(body.data.href);
 }catch(e){setError(e instanceof Error?e.message:'Try again.');setPending(false);}
 }}/>{!options.length&&<p>No active roles are available. Ask an administrator to review your access.</p>}{error&&<p role="alert">{error} <button onClick={()=>window.location.reload()}>Check again</button></p>}</div>;
}
