'use client';
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {useEffect} from 'react';
import { enqueueWorkspaceMemory } from "@erpx/feature-work_projection/lib/workspace-memory-client";

export function RememberWorkspace({base,companyId,assignmentId,revision}:{base:string;companyId?:string;assignmentId?:string;revision?:number}) {
  useEffect(()=>{
    // Save only a mounted workspace, never a prefetched server render.
    void enqueueWorkspaceMemory(() =>
      holdingFetch('/api/v1/workspace-preference',{
        method:'POST',keepalive:true,headers:{'content-type':'application/json',
          ...(companyId && assignmentId && revision!==undefined ? {
            'x-erpx-context-company':companyId,'x-erpx-context-assignment':assignmentId,'x-erpx-context-revision':String(revision),
          } : {}),
        },body:JSON.stringify({base}),
      }),
    );
  },[base,companyId,assignmentId,revision]);
  return null;
}
