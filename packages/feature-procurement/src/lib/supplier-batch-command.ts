import {workFetch,type WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
import {procurementCommandContext,procurementFormUnchanged} from "@erpx/feature-procurement/lib/command";
export type SupplierBatchReceipt=Record<string,unknown>&{id:string};
export type SupplierBatchAttempt={path:string;body:string;context:string;key:string;completion?:WorkCommandCompletion};
export function validSupplierExport(value:unknown):value is {content:string;sha256:string}{return !!value&&typeof value==="object"&&"content"in value&&typeof value.content==="string"&&value.content.length>0&&"sha256"in value&&typeof value.sha256==="string"&&/^[a-f0-9]{64}$/.test(value.sha256);}
export async function verifySupplierExport(value:unknown){if(!validSupplierExport(value))throw Error("Supplier export acknowledgement is incomplete.");const digest=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(value.content)),actual=[...new Uint8Array(digest)].map(byte=>byte.toString(16).padStart(2,"0")).join("");if(actual!==value.sha256)throw Error("The supplier export does not match its retained SHA-256. Retry the original export.");return value;}
/** Match the acknowledgement to the exact original owner operation. Export
 * receipts deliberately have their own contract rather than a record version. */
export function validSupplierBatchReceipt(attempt:SupplierBatchAttempt,value:unknown):value is SupplierBatchReceipt{
 if(!value||typeof value!=="object"||!("id"in value)||typeof value.id!=="string")return false;
 const parts=attempt.path.split("/"),input=JSON.parse(attempt.body) as {expectedVersion?:number;outcome?:string};
 if(parts[0]==="supplier-batches"&&parts.length===1)return value.id===`supplier-batch-${attempt.key}`&&"version"in value&&value.version===1&&"status"in value&&value.status==="draft";
 if(parts.length!==3)return false;
 if(value.id!==parts[1])return false;
 if(parts[0]==="supplier-batches"&&parts[2]==="export")return validSupplierExport(value)&&"rows"in value&&Number.isSafeInteger(value.rows)&&Number(value.rows)>0&&"confirmationRequired"in value&&value.confirmationRequired===true;
 if(!("version"in value)||!Number.isSafeInteger(value.version)||!Number.isSafeInteger(input.expectedVersion)||value.version!==Number(input.expectedVersion)+1||!("status"in value))return false;
 if(parts[0]==="supplier-batches")return parts[2]==="approve"?value.status==="approved":parts[2]==="cancel"&&value.status==="cancelled";
 if(parts[0]!=="supplier-instructions")return false;
 if(parts[2]==="result")return ["unknown","failed"].includes(input.outcome??"")&&value.status===input.outcome;
 return parts[2]==="confirm"&&value.status==="confirmed"&&"paymentId"in value&&value.paymentId===`supplier-payment-${attempt.key}`&&"journalId"in value&&typeof value.journalId==="string"&&!!value.journalId;
}
/** One unknown payment command must be resolved before a different financial
 * request. Known rejections retain their exact key while allowing corrections. */
export function createSupplierBatchCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 const keys=new Map<string,string>();let pending:SupplierBatchAttempt|null=null;
 const signature=(path:string,body:string,context:string)=>JSON.stringify([path,body,context]);
 return {
  prepare(path:string,input:unknown,completion?:WorkCommandCompletion,context=procurementCommandContext()){
   const body=JSON.stringify(input),stamp=signature(path,body,context);if(pending&&(pending.path!==path||pending.body!==body||pending.context!==context))throw Error("Retry the submitted supplier payment action first. Your newer draft remains here.");
   const key=keys.get(stamp)??newKey();keys.set(stamp,key);return pending??{path,body,context,key,completion};
  },
  pending:()=>pending,
  async execute(base:string,attempt:SupplierBatchAttempt,currentContext=procurementCommandContext()){
   if(attempt.context!==currentContext)throw Error("Your Company or role changed. Return to the original payment role before retrying.");
   let uncertain=true;
   try{
    const response=await send(`/api/v1${base}/finance/${attempt.path}`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body:attempt.body},attempt.completion,"caller"),envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
    if(!response.ok){uncertain=response.status>=500||envelope.error?.code==="WORK_CONTEXT_CHANGED";throw Error(envelope.error?.message??"Supplier payment action could not be saved.");}
    if(!validSupplierBatchReceipt(attempt,envelope.data))throw Error("Supplier payment acknowledgement is incomplete. Retry the submitted action.");
    if(attempt.path.endsWith("/export"))await verifySupplierExport(envelope.data);
    keys.delete(signature(attempt.path,attempt.body,attempt.context));pending=null;return {result:envelope.data,unchanged:procurementFormUnchanged(attempt.completion)};
   }catch(error){pending=uncertain?attempt:null;throw error;}
  },
 };
}
