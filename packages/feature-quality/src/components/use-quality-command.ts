"use client";
import {useRef,useState} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {qualityCommandAllowed} from "@erpx/feature-quality/lib/quality-purchase-controls";
import {captureQualityForm,qualityCommandContext,qualityCommandFetch,qualityFormUnchanged,validQualityAcknowledgement} from "@erpx/feature-quality/lib/quality-command-fetch";
import {createQualityCommandAttempt,type QualityCommandAttempt} from "@erpx/feature-quality/lib/quality-command-attempt";
import {useQualityPurchaseScope} from "@erpx/feature-quality/components/quality-purchase-context";
type Receipt={id:string;version:number};
export function useQualityCommand(base:string) {
  const router=useRouter(),context=useQualityPurchaseScope(),purchase=context.purchase,pendingRef=useRef(false),commands=useRef(createQualityCommandAttempt());
  const [error,setError]=useState(""),[notice,setNotice]=useState(""),[pending,setPending]=useState(false),[uncertain,setUncertain]=useState(false);
  async function execute(attempt:QualityCommandAttempt,retry=false) {
    if(pendingRef.current)return false;
    const requested=attempt;
    try{attempt=commands.current.select(attempt,qualityCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted quality.");return false;}
    const url=`/api/v1${base}/quality${attempt.path}`;
    const sourcePurchase=context.sources?.[attempt.path.split("/")[1]]??purchase;
    if(!qualityCommandAllowed(sourcePurchase,attempt.path,JSON.parse(attempt.body)) && !qualityCommandFetch.hasAttempt(url,attempt.body)){setError("This quality action is unavailable for the current package. Your draft is retained.");return false;}
    pendingRef.current=true;setPending(true);setError("");setNotice("");
    // A retry acknowledges the original submitted revision, never later edits.
    const completion=retry || attempt!==requested ? attempt.completion:captureQualityForm();attempt.completion=completion;
    let mayHaveCommitted=true;
    try {
      const response=await qualityCommandFetch(url,{method:"POST",headers:{"Content-Type":"application/json"},body:attempt.body},completion);
      const result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
      if(!response.ok){mayHaveCommitted=response.status>=500 || result.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved(attempt);setUncertain(false);}throw Error(result.error?.message ?? "Unable to save quality.");}
      if(!validQualityAcknowledgement(result.data))throw Error("Quality acknowledgement is incomplete. Retry the submitted change.");
      commands.current.resolved(attempt);setUncertain(false);attempt.acknowledged?.(result.data);
      if(!qualityFormUnchanged(completion)){setNotice("Submitted quality change saved. Your newer draft is retained.");return false;}
      if(attempt.create)router.push(canonicalUiHref(`${base}/quality/${result.data.id}`));
      router.refresh();if(completion&&typeof window!=="undefined")window.dispatchEvent(new CustomEvent("erpx:quality-command-complete",{detail:completion}));return true;
    } catch(cause) {
      if(mayHaveCommitted && qualityCommandFetch.hasAttempt(url,attempt.body) && !commands.current.retry()){commands.current.unresolved(attempt);setUncertain(true);}
      setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted quality change.");return false;
    } finally {pendingRef.current=false;setPending(false);}
  }
  const send=(path:string,input:unknown,create=false,acknowledged?:(receipt:Receipt)=>void)=>execute({path,body:JSON.stringify(input),create,context:qualityCommandContext(),acknowledged});
  const retry=()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve(false);};
  return {send,retry,error,notice,pending,uncertain,purchase,canPrepare:purchase?.newWork!==false || uncertain,canSettle:purchase?.settleExisting!==false || uncertain};
}
