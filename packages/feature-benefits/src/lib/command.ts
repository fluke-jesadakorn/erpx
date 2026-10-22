import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {procurementCommandContext,procurementFormUnchanged} from '@erpx/feature-procurement/lib/command';
export type BenefitKind='plan'|'window'|'dependent'|'election';
export type BenefitReceipt={id:string;version:number;status:string};
export type BenefitAttempt={input:Readonly<Record<string,unknown>>;body:string;key:string;context:string;completion?:WorkCommandCompletion};
const states:Record<BenefitKind,Record<string,string>>={plan:{publish:'published',reject:'rejected',retire:'retired'},window:{open:'open',close:'closed',cancel:'cancelled'},dependent:{verify:'verified',reject:'rejected',remove:'removed'},election:{submit:'submitted',acknowledge:'acknowledged',release:'released',cancel:'cancelled'}};
export function validBenefitReceipt(attempt:BenefitAttempt,value:unknown):value is BenefitReceipt{
 if(!value||typeof value!=='object'||Array.isArray(value)||!('id'in value)||!('version'in value)||!('status'in value)||typeof value.id!=='string'||typeof value.status!=='string'||!Number.isSafeInteger(value.version))return false;
 const input=attempt.input,kind=input.entity;if(kind!=='plan'&&kind!=='window'&&kind!=='dependent'&&kind!=='election')return false;
 if(typeof input.id==='string')return Number.isSafeInteger(input.expectedVersion)&&Number(input.expectedVersion)>0&&value.id===input.id&&value.version===Number(input.expectedVersion)+1&&value.status===states[kind][String(input.action)];
 return value.id===`benefit-${kind}-${attempt.key}`&&value.version===1&&value.status===(kind==='dependent'?'pending':kind==='election'?'elected':'draft');
}
export function benefitCommandContext(rendered:string){return JSON.stringify([rendered,procurementCommandContext()]);}
/** The retained request is an exact submitted body/key/context, never a newer draft. */
export function createBenefitCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 const keys=new Map<string,string>();let pending:BenefitAttempt|null=null;
 const stamp=(body:string,context:string)=>JSON.stringify([body,context]);
 return {pending:()=>pending,prepare(input:Record<string,unknown>,form?:HTMLFormElement,context='server'){
  const body=JSON.stringify(input);if(pending&&(pending.body!==body||pending.context!==context))throw Error('Retry the submitted Benefits action first. Your newer draft remains here.');
  const signature=stamp(body,context),key=keys.get(signature)??newKey();keys.set(signature,key);
  return pending??{input:Object.freeze(JSON.parse(body) as Record<string,unknown>),body,key,context,completion:form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined};
 },async execute(base:string,attempt:BenefitAttempt,currentContext:()=>string){
  if(attempt.context!==currentContext())throw Error('Your account, Company or role changed. Return to the original Benefits context before retrying.');
  const alreadyUncertain=pending!==null;let uncertain=true;
  try{const response=await send(`/api/v1${base}/benefits`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},attempt.completion,'caller');
   const envelope=await response.json() as {data?:unknown;error?:{message?:string;code?:string}};
   if(attempt.context!==currentContext())throw Error('Your account, Company or role changed. The submitted action remains available for review in its original context.');
   if(!response.ok){uncertain=alreadyUncertain||response.status>=500||['WORK_CONTEXT_CHANGED','WORK_ROLE_REQUIRED','WORK_ROLE_UNAVAILABLE'].includes(envelope.error?.code??'');throw Error(envelope.error?.message??'Unable to save the Benefits action.');}
   if(!validBenefitReceipt(attempt,envelope.data))throw Error('Benefits acknowledgement is incomplete. Retry the submitted action.');
   const {id,version,status}=envelope.data;keys.delete(stamp(attempt.body,attempt.context));pending=null;
   return {result:{id,version,status},unchanged:procurementFormUnchanged(attempt.completion)};
  }catch(error){pending=uncertain?attempt:null;throw error;}
 }};
}
