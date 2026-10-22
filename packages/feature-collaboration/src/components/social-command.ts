"use client";
import {useLayoutEffect,useRef,useState} from 'react';
import {betaFetch} from '@erpx/feature-shared/lib/beta-client';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {createSocialImageCommands,createSocialCommands,socialCommandContext,type SocialAttempt} from '@erpx/feature-collaboration/lib/social-command';
import {useSocialRenderedContext} from '@erpx/feature-collaboration/components/social-command-context';
export function useSocialCommand(){
 const rendered=useSocialRenderedContext(),context=useRef(rendered);useLayoutEffect(()=>{context.current=rendered;},[rendered]);
 const images=useRef(createSocialImageCommands()),[imagePending,setImagePending]=useState(false);
 const commands=useRef(createSocialCommands()),attempts=useRef(new Map<string,string>()),running=useRef(false),[busy,setBusy]=useState(false),[message,setMessage]=useState(''),[retryAttempt,setRetryAttempt]=useState<SocialAttempt|null>(null),[recovered,setRecovered]=useState<unknown>(null),[retrying,setRetrying]=useState(false);
 const stamp=()=>socialCommandContext(context.current?.token??'');
 async function execute(attempt:SocialAttempt){try{const reply=await commands.current.execute(attempt,stamp,context.current??undefined);setRetryAttempt(null);announceWorkChange();return reply;}catch(error){setRetryAttempt(commands.current.pending());throw error;}}
 async function send<T>(url:string,input?:unknown,form?:HTMLFormElement):Promise<T>{
  if(input===undefined)return betaFetch<T>(url);
  // The existing Identity access-revocation consumer has its own owner and
  // retains its original transport when no Social context was supplied.
  if(!context.current){const fingerprint=JSON.stringify({url,input}),key=attempts.current.get(fingerprint)??crypto.randomUUID();attempts.current.set(fingerprint,key);const value=await betaFetch<T>(url,input,key);attempts.current.delete(fingerprint);return value;}
  const activeForm=form??(document.activeElement?.closest('form')??undefined),attempt=commands.current.prepare(url,input,stamp(),activeForm);let reply:Awaited<ReturnType<typeof execute>>;try{reply=await execute(attempt);}catch(error){if(commands.current.pending()!==attempt)throw error;setMessage(error instanceof Error?error.message:'Retry the submitted conversation action.');reply=await commands.current.wait(attempt);}const {result,unchanged}=reply;if(!unchanged){setRecovered(result);throw Error('The submitted action was saved. Your newer draft remains here for review.');}if(activeForm&&(/\/posts\/[^/]+(?:\/task)?$/.test(url)||/\/beta\/drafts\/[^/]+\/publish$/.test(url)))window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form:activeForm,formEditVersion:activeForm.getAttribute('data-erpx-edit-version'),socialAcknowledged:true}}));return result as T;
 }
 async function uploadImage(url:string,file:File){
  const root=context.current?.apiRoot??'',attempt=images.current.prepare(url,file,stamp());
  if(!context.current)throw Error('Open this image in its original conversation context.');
  try{return await images.current.execute(attempt,stamp,root);}catch(error){setImagePending(!!images.current.pending());if(!images.current.pending())throw error;setMessage(error instanceof Error?error.message:'Retry the original image.');return images.current.wait();}
 }
 async function run(action:()=>Promise<void>){if(running.current)return;running.current=true;setBusy(true);setMessage('');try{await action();}catch(error){setMessage(error instanceof Error?error.message:'Please try again.');}finally{running.current=false;setBusy(false);}}
 const retry=async()=>{if(retrying||!retryAttempt&&!imagePending)return;setRetrying(true);try{const image=images.current.pending();if(image){await images.current.execute(image,stamp,context.current?.apiRoot??'');setImagePending(false);}else if(retryAttempt)await execute(retryAttempt);setMessage('The original submitted action was acknowledged. Continuing that same action.');}catch(error){setMessage(error instanceof Error?error.message:'Retry the submitted conversation action.');}finally{setRetrying(false);}};
 return {run,send,uploadImage,busy,message,setMessage,retry,retryAttempt,retrying,imagePending,recovered};
}
