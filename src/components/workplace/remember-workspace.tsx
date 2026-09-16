'use client';
import {useEffect} from 'react';

export function RememberWorkspace({base,companyId,assignmentId,revision}:{base:string;companyId?:string;assignmentId?:string;revision?:number}) {
  useEffect(()=>{
    // Save only a mounted workspace, never a prefetched server render.
    void fetch('/api/v1/workspace-preference',{
      method:'POST',keepalive:true,headers:{'content-type':'application/json',
        ...(companyId && assignmentId && revision!==undefined ? {
          'x-erpx-context-company':companyId,'x-erpx-context-assignment':assignmentId,'x-erpx-context-revision':String(revision),
        } : {}),
      },body:JSON.stringify({base}),
    }).catch(()=>{/* Role/company selection is also saved by its server endpoint. */});
  },[base,companyId,assignmentId,revision]);
  return null;
}
