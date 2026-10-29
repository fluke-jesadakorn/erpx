import {confirmsWithdrawal,type WithdrawalAttempt,type WithdrawalStatus} from '../domain/field-withdrawal';

type Context = {identity:string;generation:number;valid:boolean};
type Input = {scope:string;base:string;linkId:string;setId:string;reason:string;draftReason:string};
type Ports = {context:()=>Context|null;key:()=>string;withdraw:(attempt:WithdrawalAttempt)=>Promise<unknown>;acknowledge:()=>void};

/** Coordinates one original command until its actual owner response is confirmed. */
export function createFieldWithdrawal(ports:Ports) {
 const attempts=new Map<string,WithdrawalAttempt>(),listeners=new Map<string,Set<()=>void>>(),owners=new Map<string,()=>{setId:string|null}>();
 const changed=(scope:string)=>{for(const listener of listeners.get(scope)??[])listener();};
 function current(a:WithdrawalAttempt){const context=ports.context();if(!owners.has(a.scope)||!context?.valid||context.generation!==a.generation||context.identity!==a.identity)throw Error('Your account, Company or role changed. Return to the original work context before retrying.');}
 function register(scope:string,owner:()=>{setId:string|null}){owners.set(scope,owner);return()=>{if(owners.get(scope)===owner)owners.delete(scope);};}
 function pending(scope:string){const attempt=attempts.get(scope),context=ports.context();return attempt&&context&&context.identity===attempt.identity&&context.generation===attempt.generation?attempt:undefined;}
 function subscribe(scope:string,listener:()=>void){let set=listeners.get(scope);if(!set){set=new Set();listeners.set(scope,set);}set.add(listener);return()=>{set.delete(listener);if(!set.size)listeners.delete(scope);};}
 async function run(attempt:WithdrawalAttempt):Promise<WithdrawalStatus>{if(attempt.busy)throw Error('Wait for the original withdrawal.');current(attempt);const originalOwner=owners.get(attempt.scope);attempt.busy=true;changed(attempt.scope);try{const status=await ports.withdraw(attempt);current(attempt);if(owners.get(attempt.scope)!==originalOwner)throw Error('Return to the original withdrawal and retry confirmation.');if(!confirmsWithdrawal(status,attempt))throw Error('Unable to confirm the original withdrawal.');if(attempts.get(attempt.scope)!==attempt)throw Error('Return to the original withdrawal.');attempts.delete(attempt.scope);changed(attempt.scope);ports.acknowledge();return status;}finally{attempt.busy=false;changed(attempt.scope);}}
 async function submit(input:Input):Promise<WithdrawalStatus>{let attempt=attempts.get(input.scope);if(attempt){if(attempt.linkId!==input.linkId||attempt.setId!==input.setId||attempt.reason!==input.reason)throw Error('Confirm the original withdrawal first. Your newer entries remain unsent.');}else{const context=ports.context();if(!context?.valid)throw Error('Review your current work role before withdrawing this file.');attempt={...input,key:ports.key(),generation:context.generation,identity:context.identity,busy:false};attempts.set(input.scope,attempt);changed(input.scope);}return run(attempt);}
 async function retry(scope:string):Promise<WithdrawalStatus>{const attempt=attempts.get(scope);if(!attempt)throw Error('There is no original withdrawal to confirm.');return run(attempt);}
 return {register,pending,subscribe,submit,retry};
}
