"use client";
import {useEffect,useLayoutEffect,useMemo,useRef,useState} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {registerUnsavedWork,registerContextSwitchBlocker} from "@erpx/feature-work_projection/lib/context-switch";
import {createFieldServiceCommands,fieldServiceOwner,type FieldServiceSource} from "@erpx/feature-field_service/lib/command";
import {createNativeReserveCommands,type ReserveContext} from '@erpx/feature-field_service/lib/native-reserve-command';
export function useFieldServiceCommand(current:()=>FieldServiceSource|null,rendered:FieldServiceSource,nativeReserve?:{context?:ReserveContext;issue?:boolean;reserve?:boolean;request?:boolean}) {
  const router=useRouter(),commands=useRef<ReturnType<typeof createFieldServiceCommands>|null>(null),inFlight=useRef(false);commands.current??=createFieldServiceCommands(current);
  const native=useMemo(()=>createNativeReserveCommands(current,undefined,JSON.stringify([rendered.base,rendered.recordId])),[current,rendered.base,rendered.recordId]);
  useLayoutEffect(()=>{native.setContext(nativeReserve?.context);},[native,nativeReserve?.context]);
  const [messageOwner,setMessageOwner]=useState<string|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(""),[uncertain,setUncertain]=useState(()=>native.pending()),[legacyPending,setLegacyPending]=useState(false),[notice,setNotice]=useState(""),[saved,setSaved]=useState<{owner:string;id:string;href:string;agreement:boolean}|null>(null);
  useEffect(()=>native.subscribe(()=>setUncertain(native.pending() || commands.current!.pending())),[native]);
  const visibleUncertain=native.pending() || legacyPending || uncertain && messageOwner===fieldServiceOwner(rendered);
  useEffect(()=>registerUnsavedWork(()=>busy||visibleUncertain),[busy,visibleUncertain]);
  useEffect(()=>registerContextSwitchBlocker(()=>native.pending()?'The original Service request awaits confirmation. Retry it before switching work context.':null),[native]);
  async function send(input:unknown,form?:HTMLFormElement,retry=false,displayed?:FieldServiceSource) {
    if(inFlight.current)return;const source=current();if(!source)return;const capturedOwner=fieldServiceOwner(source);inFlight.current=true;setMessageOwner(capturedOwner);setBusy(true);setError("");setNotice("");
    try {
      native.setContext(nativeReserve?.context);
      const reserve=native.pending() || !!nativeReserve && !retry && !!input && typeof input==='object' && (('entity' in input && input.entity==='request' && nativeReserve.request) || ('action' in input && (input.action==='reserve-part' && nativeReserve.reserve!==false || input.action==='issue-part' && nativeReserve.issue)));
      if(reserve && commands.current!.pending() && !native.pending()) throw Error("Confirm the original Service request first. Your newer entries remain unsent.");
      const selected=reserve?native:commands.current!;
      const outcome=await(retry?selected.retry():selected.submit(input,form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined,displayed)),unchanged=outcome.unchanged();
      if(!outcome.acknowledge())throw Error("Confirm the original Service request again.");setUncertain(false);setLegacyPending(commands.current!.pending());
      const href=canonicalUiHref(`${source.base}/field-service${outcome.agreement?'':`/${encodeURIComponent(outcome.receipt.id)}`}`);setSaved({owner:capturedOwner,id:outcome.receipt.id,href,agreement:outcome.agreement});setNotice(unchanged?"The original Service request was saved.":"The original Service request was saved. Your newer entries remain unsent.");
      if(unchanged){outcome.completion()?.form.reset();try{router.push(href);router.refresh();}catch{setError("Saved, but navigation failed. Open the saved original record without submitting a replacement request.");}}
    } catch(cause) {setLegacyPending(commands.current!.pending());setUncertain(native.pending() || commands.current!.pending());setError(cause instanceof Error?cause.message:"The result is uncertain. Confirm the original Service request.");}
    finally {inFlight.current=false;setBusy(false);}
  }
  const visible=saved&&saved.owner===fieldServiceOwner(rendered)?saved:null;
  const sameOwner=messageOwner===fieldServiceOwner(rendered);
  return {send,busy,error:sameOwner?error:"",uncertain:visibleUncertain,nativePending:native.pending(),readbackPending:native.pending()&&native.saved(),notice:sameOwner?notice:"",saved:visible,retry:()=>send(undefined,undefined,true),clearError:()=>setError(""),fail:(cause:unknown)=>{setMessageOwner(fieldServiceOwner(rendered));setError(cause instanceof Error?cause.message:"Review the form values.");}};
}
