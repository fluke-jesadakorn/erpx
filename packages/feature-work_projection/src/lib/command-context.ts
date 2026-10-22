import {contextGeneration,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';

const boundary=()=>typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
const identity=(node:HTMLElement|null)=>node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(':'):null;

/** Recheck the rendered authority after asynchronous acknowledgement decoding.
 * Owners still validate their own source, body/key and receipt before clearing. */
export function captureWorkCommandContext(){
 const generation=contextGeneration(),node=boundary(),stamp=identity(node);
 return ()=>{
  const current=boundary();
  if(generation!==contextGeneration()||contextIsSwitching()||node?.isConnected===false||node?.dataset.workStale==='true'||current?.isConnected===false||current?.dataset.workStale==='true'||stamp!==identity(current))
   throw Object.assign(new Error('Your account, Company or role changed. Return to the original work context before retrying.'),{code:'WORK_CONTEXT_CHANGED'});
 };
}
