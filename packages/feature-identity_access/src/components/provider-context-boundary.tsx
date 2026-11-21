'use client';
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {createContext,useContext,useEffect,useState,useSyncExternalStore} from 'react';
import {Button} from '@erpx/ui/components/foundation';
import {InlineNotice} from '@erpx/ui/components/patterns';
import {useContextSwitch} from '@erpx/workplace/components/context-switch-context';
import {contextGeneration,contextIsSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {createProviderContextClient,providerContextBlocked,type ProviderContextState} from '@erpx/feature-identity_access/lib/provider-context-client';
import type {ProviderContextSeed} from '@erpx/feature-identity_access/lib/provider-context';
import {useAccountPreferences} from '@erpx/feature-identity_access/components/account-preferences-context';
const Context=createContext<{state:ProviderContextState;select:(id:string)=>Promise<boolean>}|null>(null);
async function parsed(response:Response){const body=await response.json();if(!response.ok)throw Object.assign(Error(body?.error?.message??'The Platform context could not be read.'),{status:response.status});return body?.data;}
function model(initial:ProviderContextSeed){return createProviderContextClient(initial,{read:signal=>holdingFetch(`/api/v1/platform/context?accountKey=${encodeURIComponent(initial.accountKey)}`,{cache:'no-store',signal}).then(parsed),write:(input,key,signal)=>holdingFetch('/api/v1/platform/context',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':key},body:JSON.stringify(input),signal}).then(parsed),key:()=>crypto.randomUUID(),generation:contextGeneration});}
export function ProviderContextBoundary({initial,children}:{initial:ProviderContextSeed;children:React.ReactNode}){
 const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,en:string)=>th?thai:en;
 const {switchContext}=useContextSwitch(),[holder,setHolder]=useState(()=>({key:initial.accountKey,client:model(initial)}));
 if(holder.key!==initial.accountKey)setHolder({key:initial.accountKey,client:model(initial)});
 const client=holder.client;
 const state=useSyncExternalStore(client.subscribe,client.getSnapshot,client.getSnapshot);
 useEffect(()=>{client.activate();client.observe(initial);},[client,initial]);
 useEffect(()=>{client.activate();let away=false;const blur=()=>{away=true;},focus=()=>{if(!away||document.visibilityState==='hidden'||contextIsSwitching())return;away=false;void client.refresh();},visibility=()=>{if(document.visibilityState==='hidden')blur();else focus();},peer=()=>{if(!contextIsSwitching()&&document.visibilityState!=='hidden')void client.refresh();},channel=typeof BroadcastChannel==='undefined'?null:new BroadcastChannel('erpx-work-context');channel?.addEventListener('message',peer);window.addEventListener('blur',blur);window.addEventListener('focus',focus);document.addEventListener('visibilitychange',visibility);window.addEventListener('erpx:work-context-changed',peer);return()=>{client.dispose();channel?.close();window.removeEventListener('blur',blur);window.removeEventListener('focus',focus);document.removeEventListener('visibilitychange',visibility);window.removeEventListener('erpx:work-context-changed',peer);};},[client]);
 const run=(action:()=>Promise<void>)=>switchContext({inspect:false,action});
 return <Context.Provider value={{state,select:id=>run(()=>client.select(id))}}>{state.error&&<InlineNotice tone='danger' role='alert'>{state.error}</InlineNotice>}{state.retry&&<Button type='button' appearance='standard' disabled={state.pending} onClick={()=>{void run(()=>client.retry()).catch(()=>{});}}>{t('ลองเลือกบทบาทเดิมอีกครั้ง','Retry captured role selection')}</Button>}{state.stale&&<Button type='button' appearance='standard' disabled={state.pending} onClick={()=>{void run(async()=>{await client.refresh();}).catch(()=>{});}}>{t('ตรวจสอบบริบทแพลตฟอร์มปัจจุบัน','Review current Platform context')}</Button>}<div hidden={providerContextBlocked(state)} inert={providerContextBlocked(state)}>{children}</div></Context.Provider>;
}
export function useProviderContext(){const value=useContext(Context);if(!value)throw Error('ProviderContextBoundary is required.');return value;}
