import {workFetch} from "@/lib/work_projection/work-fetch";
import type {TalentAttempt} from "./command";
export function validHireReceipt(attempt:TalentAttempt,value:unknown):value is {id:string;handoffId:string;personId:string;engagementId:string;compensationId:string;status:"consumed";version:1;reconciled:boolean}{
 if(!value||typeof value!=="object"||Array.isArray(value))return false;const result=value as Record<string,unknown>,input=JSON.parse(attempt.body) as {handoffId:string;reconcilePersonId?:string};
 return result.id===`hire-consumption-${attempt.key}`&&result.handoffId===input.handoffId&&result.status==="consumed"&&result.version===1&&result.personId===(input.reconcilePersonId??`person-hire-${attempt.key}`)&&result.engagementId===`engagement-hire-${attempt.key}`&&result.compensationId===`compensation-hire-${attempt.key}`&&result.reconciled===!!input.reconcilePersonId;
}
export function sendTalentHireCommand(base:string,attempt:TalentAttempt,send:typeof workFetch=workFetch){return send(`/api/v1${base}/people/hire-handoffs/consume`,{method:"POST",headers:{"Content-Type":"application/json","Idempotency-Key":attempt.key},body:attempt.body},attempt.completion,"caller");}
