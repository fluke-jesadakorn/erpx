import {confirmsBind,type BindAttempt,type BindResult} from '../domain/field-bind';
type Ports={context:()=>{identity:string;generation:number;valid:boolean}|null;key:()=>string;bind:(attempt:BindAttempt)=>Promise<unknown>;acknowledge:()=>void};
export function createFieldBind(ports:Ports){
 const attempts=new Map<string,BindAttempt>(),owners=new Map<string,object>(),listeners=new Map<string,Set<()=>void>>();
 const changed=(scope:string)=>{for(const listener of listeners.get(scope)??[])listener();};
 function register(scope:string,owner:object){owners.set(scope,owner);return()=>{if(owners.get(scope)===owner)owners.delete(scope);};}
 function subscribe(scope:string,listener:()=>void){let set=listeners.get(scope);if(!set){set=new Set();listeners.set(scope,set);}set.add(listener);return()=>{set.delete(listener);if(!set.size)listeners.delete(scope);};}
 function pending(scope:string){const a=attempts.get(scope),c=ports.context();return a&&c&&a.identity===c.identity&&a.generation===c.generation?a:undefined;}
 function current(a:BindAttempt){const c=ports.context();if(!owners.has(a.scope)||!c?.valid||a.identity!==c.identity||a.generation!==c.generation)throw Error('Your account, Company or role changed. Return to the original work context before retrying.');}
 async function run(a:BindAttempt):Promise<BindResult>{if(a.busy)throw Error('Wait for the original file binding.');current(a);const owner=owners.get(a.scope);a.busy=true;changed(a.scope);try{const result=await ports.bind(a);current(a);if(owners.get(a.scope)!==owner)throw Error('Return to the original file binding and retry confirmation.');if(!confirmsBind(result,a))throw Error('Unable to confirm the original file binding.');if(attempts.get(a.scope)!==a)throw Error('Return to the original file binding.');attempts.delete(a.scope);changed(a.scope);ports.acknowledge();return result;}finally{a.busy=false;changed(a.scope);}}
 async function submit(input:{scope:string;base:string;setId:string;expectedVersion:number}){let a=attempts.get(input.scope);if(a){if(a.setId!==input.setId||a.expectedVersion!==input.expectedVersion)throw Error('Confirm the original file binding first.');}else{const c=ports.context();if(!c?.valid)throw Error('Review your current work role before binding files.');a={...input,key:ports.key(),identity:c.identity,generation:c.generation,busy:false};attempts.set(input.scope,a);changed(input.scope);}return run(a);}
 async function retry(scope:string){const a=attempts.get(scope);if(!a)throw Error('There is no original file binding to confirm.');return run(a);}
 return {register,subscribe,pending,submit,retry};
}
