import type {PublicMarketingReceipt} from '@/server/modules/marketing/public/public-submission';

export type PublicMarketingAttempt=Readonly<{key:string;body:string;context:string;generation:number;editVersion:number}>;
const object=(value:unknown):value is Record<string,unknown>=>value!==null&&typeof value==='object'&&!Array.isArray(value);
export function validPublicMarketingReceipt(value:unknown):value is PublicMarketingReceipt{return object(value)&&typeof value.id==='string'&&value.id.trim().length>0&&value.status==='pending';}
export function publicMarketingAttemptCurrent(attempt:PublicMarketingAttempt,context:string,generation:number){return attempt.context===context&&attempt.generation===generation;}
export function publicMarketingAcknowledgementCurrent(attempt:PublicMarketingAttempt,context:string,generation:number,editVersion:number){return publicMarketingAttemptCurrent(attempt,context,generation)&&attempt.editVersion===editVersion;}

/** Unknown results retain the exact submitted body/key. A later edit cannot
 * silently be sent under that key or acknowledged by its older response. */
export function createPublicMarketingCommand(nextKey:()=>string=()=>crypto.randomUUID()){
 let pending:PublicMarketingAttempt|null=null;
 return {
  select(body:string,context:string,generation:number,editVersion:number){
   if(pending){if(!publicMarketingAttemptCurrent(pending,context,generation))throw Error('Return to the original form and account context before retrying.');if(pending.body!==body)throw Error('Retry the submitted request first. Your newer details are still here.');return pending;}
   return Object.freeze({key:nextKey(),body,context,generation,editVersion});
  },
  retry(){return pending;},
  async execute(attempt:PublicMarketingAttempt,send:(attempt:PublicMarketingAttempt)=>Promise<Response>,current:()=>boolean):Promise<PublicMarketingReceipt>{
   if(!current())throw Error('Return to the original form and account context before retrying.');
   let uncertain=true;
   try{
    const response=await send(attempt),value:unknown=await response.json();
    if(!response.ok){uncertain=response.status>=500||pending===attempt;const message=object(value)&&object(value.error)&&typeof value.error.message==='string'?value.error.message:'The request could not be submitted.';throw Error(message);}
    const receipt=object(value)?value.data:undefined;
    if(!validPublicMarketingReceipt(receipt))throw Error('The acknowledgement is incomplete. Retry the exact submitted request.');
    if(!current())throw Error('The form or account context changed. Return to the original context to review the submitted request.');
    if(pending===attempt)pending=null;
    return receipt;
   }catch(error){if(uncertain)pending=attempt;throw error;}
  },
 };
}
