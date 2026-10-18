import {workFetch,type WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
export type ProjectCommandAcknowledgement=Record<string,unknown>&{id:string;version:number};
export function projectCommandAcknowledgement(value:unknown):ProjectCommandAcknowledgement{
 if(!value||typeof value!=="object"||!("id"in value)||typeof value.id!=="string"||!value.id||!("version"in value)||!Number.isSafeInteger(value.version)||Number(value.version)<1)throw Error("The Project acknowledgement is incomplete. Keep your draft and retry the same action.");
 return value as ProjectCommandAcknowledgement;
}
/** The receipt key belongs to the exact path and body. Transport failure,
 * purchase denial and incomplete acknowledgements retain that request. */
export function createProjectCommandFetch(fetcher:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let attempt:{signature:string;key:string}|null=null;
 return async(url:string,input:unknown,completion?:WorkCommandCompletion)=>{
  const body=JSON.stringify(input),signature=url+"\0"+body;if(attempt?.signature!==signature)attempt={signature,key:newKey()};
  const response=await fetcher(url,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body},completion,"caller"),envelope=await response.json() as {data?:unknown;error?:{message?:string}};
  if(!response.ok)throw Error(envelope.error?.message??"Unable to save. Your Project draft is still here.");
  const result=projectCommandAcknowledgement(envelope.data);attempt=null;return result;
 };
}
export function projectFormUnchanged(completion?:WorkCommandCompletion){return !completion||completion.formEditVersion===completion.form.getAttribute("data-erpx-edit-version");}
