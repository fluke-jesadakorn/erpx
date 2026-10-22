"use client";
import {captureWorkCommandContext} from "@erpx/feature-work_projection/lib/command-context";
import {useRef,useState} from "react";
import {useRouter} from "@erpx/feature-shared/lib/destination-router";
import {canonicalUiHref} from "@erpx/feature-work_projection/lib/destination-registry";
import {manufacturingCommandAllowed,type ManufacturingPurchaseControls} from "@erpx/feature-manufacturing/lib/purchase-controls";
import {captureManufacturingForm,manufacturingCommandContext,manufacturingCommandFetch,manufacturingFormUnchanged,validManufacturingAcknowledgement} from "@erpx/feature-manufacturing/lib/command-fetch";
import {createManufacturingCommandAttempt,type ManufacturingCommandAttempt} from "@erpx/feature-manufacturing/lib/command-attempt";
export function useProductionCommand(base:string,purchase?:ManufacturingPurchaseControls,recordPurchases?:Record<string,ManufacturingPurchaseControls>,advanced?:ManufacturingPurchaseControls){
 const router=useRouter(),pendingRef=useRef(false),commands=useRef(createManufacturingCommandAttempt());
 const [error,setError]=useState(""),[notice,setNotice]=useState(""),[pending,setPending]=useState(false),[uncertain,setUncertain]=useState(false);
 async function execute(attempt:ManufacturingCommandAttempt,retry=false){
 if(pendingRef.current)return;const requested=attempt,wasUncertain=commands.current.retry()!==null;
 try{attempt=commands.current.select(attempt,manufacturingCommandContext());}catch(cause){setError(cause instanceof Error?cause.message:"Review the submitted production change.");return;}
 const url=`/api/v1${base}/production`,input=JSON.parse(attempt.body) as Record<string,unknown>,controls=typeof input.id==="string"?recordPurchases?.[input.id]??purchase:purchase;
 if((!manufacturingCommandAllowed(controls,input)||input.comparisonScenarioId&&advanced?.newWork===false)&&!manufacturingCommandFetch.hasAttempt(url,attempt.body)){setError("This production action is unavailable for the current package. Your draft is retained.");return;}
 pendingRef.current=true;setPending(true);setError("");setNotice("");const completion=retry||attempt!==requested?attempt.completion:captureManufacturingForm();attempt.completion=completion;let mayHaveCommitted=true;
 try{const assertCurrent=captureWorkCommandContext(),response=await manufacturingCommandFetch(url,{method:"POST",headers:{"Content-Type":"application/json"},body:attempt.body},completion,"caller"),result=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
 if(!response.ok){mayHaveCommitted=wasUncertain||response.status>=500||result.error?.code==="WORK_CONTEXT_CHANGED";if(mayHaveCommitted){commands.current.unresolved(attempt);setUncertain(true);}else{commands.current.resolved(attempt);setUncertain(false);}throw Error(result.error?.message??"Unable to update production.");}
 if(!validManufacturingAcknowledgement(result.data,input))throw Error("The production acknowledgement is incomplete. Retry the submitted change.");assertCurrent();
 if(!manufacturingCommandFetch.acknowledge(response))throw Error("The production acknowledgement is incomplete. Retry the submitted change.");
 mayHaveCommitted=false;commands.current.resolved(attempt);setUncertain(false);
 if(!manufacturingFormUnchanged(completion)||!completion&&captureManufacturingForm()){setNotice("Submitted production change saved. Your newer draft is retained.");return;}
 if(completion)window.dispatchEvent(new CustomEvent("erpx:manufacturing-command-accepted",{detail:completion}));
 if(typeof result.data.orderId==="string"&&result.data.orderId.length>0)router.push(canonicalUiHref(`${base}/manufacturing/${result.data.orderId}`));router.refresh();
 }catch(cause){if(mayHaveCommitted&&!commands.current.retry()){commands.current.unresolved(attempt);setUncertain(true);}setError(cause instanceof Error?cause.message:"Connection interrupted. Retry the submitted production change.");}
 finally{pendingRef.current=false;setPending(false);}
 }
 return {send:(input:unknown)=>execute({path:"",body:JSON.stringify(input),create:false,context:manufacturingCommandContext()}),retry:()=>{const attempt=commands.current.retry();return attempt?execute(attempt,true):Promise.resolve();},error,notice,pending,uncertain};
}
