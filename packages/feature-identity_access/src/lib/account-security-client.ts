import type {AccountSecurityInput,AccountSecurityRevoked} from '@erpx/contracts/account-auth';

export function accountSecurityAcknowledgement(value:unknown):value is AccountSecurityRevoked&{revoked:true}{
 return !!value&&typeof value==='object'&&Object.keys(value).length===3&&'revoked'in value&&value.revoked===true&&'count'in value&&typeof value.count==='number'&&Number.isSafeInteger(value.count)&&value.count>=0&&'reauthenticate'in value&&typeof value.reauthenticate==='boolean';
}
/** Unknown responses retain the exact captured body and command key. This
 * handle conveys no session credential or authority. */
export function createAccountSecurityCommand(send:(input:RequestInfo|URL,init?:RequestInit)=>Promise<Response>=fetch){
 let pending:{body:string;key:string}|null=null;
 return {hasPending:()=>pending!==null,
  async revoke(input:AccountSecurityInput){
   const body=JSON.stringify(input);if(pending&&pending.body!==body)throw Error('Retry the original uncertain security request before choosing another action.');
   const attempt=pending??{body,key:crypto.randomUUID()};pending=attempt;
   let response:Response;try{response=await send('/api/v1/account/security',{method:'POST',credentials:'include',cache:'no-store',redirect:'error',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body});}catch{throw Error('The result is unknown. Retry the original security request.');}
   let value:unknown;try{value=await response.json();}catch{throw Error('The result is unknown. Retry the original security request.');}
   if(!response.ok){pending=null;const message=value&&typeof value==='object'&&'error'in value&&value.error&&typeof value.error==='object'&&'message'in value.error&&typeof value.error.message==='string'?value.error.message:'The security request was rejected.';throw Error(message);}
   if(!value||typeof value!=='object'||!('data'in value)||!accountSecurityAcknowledgement(value.data))throw Error('The result is unknown. Retry the original security request.');
   pending=null;return value.data;
  },
 };
}
