import {workFetch,type WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
export type ProcurementReceipt={id:string;version:number;promiseId?:string};
export function procurementCommandContext(){const node=typeof document==="undefined"?null:document.querySelector<HTMLElement>("[data-work-revision]");return node?[node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment].join(":"):"server";}
export function procurementFormUnchanged(completion:WorkCommandCompletion|undefined){return !completion||completion.form.isConnected&&completion.form.getAttribute("data-erpx-edit-version")===completion.formEditVersion;}
export function validProcurementReceipt(path:string,value:unknown,key?:string,expectedVersion?:number):value is ProcurementReceipt{
 if(!value||typeof value!=="object"||Array.isArray(value)||!("id" in value)||typeof value.id!=="string"||!value.id||!("version" in value)||!Number.isSafeInteger(value.version)||Number(value.version)<1)return false;
 if(expectedVersion!==undefined&&value.version!==expectedVersion+1)return false;
 const parts=path.split("/");if(parts[0]==="supplier-promises")return value.id===parts[1]&&"promiseId" in value&&typeof value.promiseId==="string"&&(key?value.promiseId===`supplier-promise-${key}`:!!value.promiseId);
 return parts[0]==="procurement-rfqs"?(parts[1]?value.id===parts[1]:key?value.id===`rfq-${key}`&&value.version===1:value.id.startsWith("rfq-")):path==="commerce/orders"&&(key?value.id===`order-${key}`&&value.version===1:value.id.startsWith("order-"));
}
export type ProcurementAttempt={path:string;body:string;order:boolean;context:string;completion?:WorkCommandCompletion;key:string};
/** An uncertain command retains its exact submitted body/key/context. Later
 * edits cannot silently become a new order or confirmation before recovery. */
export function createProcurementAttempts(){
 let pending:ProcurementAttempt|null=null;
 return {select(next:ProcurementAttempt,currentContext:string){if(next.context!==currentContext)throw Error("Your workspace changed. Review the original purchasing role before retrying.");if(pending&&(next.path!==pending.path||next.body!==pending.body||next.context!==pending.context))throw Error("Retry the submitted purchasing change first. Your newer draft is retained.");return pending??next;},unresolved(value:ProcurementAttempt){pending=value;},resolved(){pending=null;},retry(){return pending;}};
}
export function sendProcurementCommand(base:string,attempt:ProcurementAttempt,send:typeof workFetch=workFetch){return send(`/api/v1${base}/${attempt.path}`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body:attempt.body},attempt.completion,"caller");}
