"use client";
import {Button} from '@/components/ui/foundation';
import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import type {HandoverAcknowledgement as Acknowledgement} from '@/server/modules/operations/public/handover';
import type {SocialPurchase} from '@/server/modules/collaboration/public/rules/social-presentation';
import {workFetch} from '@/lib/work_projection/work-fetch';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {registerUnsavedWork} from '@/lib/work_projection/context-switch';
import {createHandoverReceiptCommands,handoverReceiptContext,handoverReceiptMatches,type HandoverAttempt} from '@/lib/work_projection/handover-acknowledgement';
import styles from './handover-composer.module.css';
import type {UiAppearance} from '@/components/ui/foundation';
export function HandoverAcknowledgement({appearance='legacy',api,postId,version,language='en',initial,contextToken='',purchase}:{appearance?:UiAppearance;api:string;postId:string;version:number;language?:'th'|'en';initial?:Acknowledgement|null;contextToken?:string;purchase?:SocialPurchase}){
 const t=(th:string,en:string)=>language==='th'?th:en,endpoint=`${api}/handover/${encodeURIComponent(postId)}/acknowledgement`,rendered=useRef(contextToken);useLayoutEffect(()=>{rendered.current=contextToken;},[contextToken]);const stamp=()=>handoverReceiptContext(rendered.current);
 const commands=useRef(createHandoverReceiptCommands()),running=useRef(false),[state,setState]=useState<Acknowledgement|null>(handoverReceiptMatches(initial,postId,version)?initial:null),[busy,setBusy]=useState(false),[error,setError]=useState(initial===null?'Confirmation status is unavailable.':''),[pending,setPending]=useState<HandoverAttempt|null>(null);
 const [previous,setPrevious]=useState({initial,postId,version,contextToken});if(previous.initial!==initial||previous.postId!==postId||previous.version!==version||previous.contextToken!==contextToken){setPrevious({initial,postId,version,contextToken});setState(handoverReceiptMatches(initial,postId,version)?initial:null);setError(initial===null?'Confirmation status is unavailable.':'');}
 useEffect(()=>registerUnsavedWork(()=>busy||pending!==null),[busy,pending]);
 useEffect(()=>{if(initial===null||handoverReceiptMatches(initial,postId,version))return;
  let active=true;const context=handoverReceiptContext(rendered.current);void workFetch(endpoint).then(async response=>{const body=await response.json() as {data?:unknown;error?:{message?:string}};if(!active||context!==handoverReceiptContext(rendered.current))return;if(!response.ok)throw Error(body.error?.message??'Confirmation status is unavailable.');if(!handoverReceiptMatches(body.data,postId,version))throw Error('This handover changed. Read its current version before confirming receipt.');setState(body.data);setError('');}).catch(cause=>{if(active)setError(cause instanceof Error?cause.message:'Confirmation status is unavailable.');});return()=>{active=false;};
 },[endpoint,initial,postId,version,contextToken]);
 async function confirm(){if(running.current)return;const old=commands.current.pending();if(old?purchase?.history===false:purchase?.settleExisting===false||!state?.canAcknowledge||state.acknowledged||state.postVersion!==version)return;running.current=true;setBusy(true);setError('');
  try{const attempt=old??commands.current.prepare(endpoint,postId,version,stamp()),reply=await commands.current.execute(attempt,stamp);setPending(null);announceWorkChange();if(reply.snapshot&&reply.postVersion===version)setState(reply.snapshot);else{const response=await workFetch(endpoint),body=await response.json() as {data?:unknown};if(!response.ok||!handoverReceiptMatches(body.data,postId,version))throw Error('Your original receipt was saved. Read the current handover version to review its status.');setState(body.data);}}
  catch(cause){setPending(commands.current.pending());setError(cause instanceof Error?cause.message:'Confirmation failed. Retry the submitted receipt.');}finally{running.current=false;setBusy(false);}
 }
 if(!state&&!error)return <p data-ui-appearance={appearance} className={styles.ack} role="status">{t('กำลังโหลดผลรับเวร…','Loading handover receipt…')}</p>;
 return <div data-ui-appearance={appearance} className={styles.ack} aria-label={t('ผลรับเวร','Handover receipt')}>
  {state&&state.postId===postId&&state.postVersion===version&&<span>{t(`รับทราบแล้ว ${state.acknowledgedCount} จาก ${state.recipientCount} คน${state.acknowledged?' · คุณรับทราบแล้ว':''}`,`${state.acknowledgedCount} of ${state.recipientCount} recipients confirmed receipt${state.acknowledged?' · You confirmed':''}.`)}</span>}
  {(pending||state?.canAcknowledge&&!state.acknowledged)&&<Button appearance={appearance} variant="plain" type="button" disabled={busy||(pending?purchase?.history===false:purchase?.settleExisting===false)} onClick={()=>void confirm()}>{busy?t('กำลังยืนยัน…','Confirming…'):pending?t('ลองยืนยันรายการเดิมอีกครั้ง','Retry submitted receipt'):t('ยืนยันรับเวร','Confirm receipt')}</Button>}
  {error&&<span role="alert">{error}</span>}
 </div>;
}
