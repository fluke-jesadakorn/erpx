"use client";
import {useEffect,useMemo,useRef,useState} from 'react';
import {createWorkforceCommandClient,type WorkforceCommand} from '@erpx/feature-people_payroll/lib/workforce-command-client';
import {workforceActionAllowed,type WorkforcePurchase} from '@erpx/feature-people_payroll/lib/workforce-controls';
import {registerUnsavedWork} from '@erpx/feature-work_projection/lib/context-switch';
export function useWorkforceCommand(base:string,context:string,purchase:WorkforcePurchase|undefined){
 const client=useMemo(()=>createWorkforceCommandClient(base,context),[base,context]),running=useRef(false);
 const [busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[pending,setPending]=useState<ReturnType<typeof client.pending>>(null);
 useEffect(()=>{const frame=requestAnimationFrame(()=>setPending(client.pending()));return()=>cancelAnimationFrame(frame);},[client]);
 useEffect(()=>registerUnsavedWork(()=>Boolean(pending)),[pending]);
 async function execute(request?:WorkforceCommand){if(running.current)return null;if(request&&!workforceActionAllowed(purchase,request.action)&&!client.pending()){setError('This Workforce action is currently unavailable. Your draft is retained.');return null;}if(!request&&purchase?.history===false){setError('Return to the original current account and role to check this action.');return null;}running.current=true;setBusy(true);setError('');setNotice('');
  try{const result=request?await client.send(request):await client.recover();if(result)setNotice('The original Workforce action was saved.');return result;}catch(error){setError(error instanceof Error?error.message:'The Workforce result is uncertain. Check the original action.');return null;}finally{running.current=false;setBusy(false);setPending(client.pending());}
 }
 return {send:execute,recover:()=>execute(),busy,error,setError,notice,setNotice,pending,allowed:(action:string)=>workforceActionAllowed(purchase,action)};
}
