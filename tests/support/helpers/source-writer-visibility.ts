import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Hold an actual native writer until the owner reader demonstrably waits for
 * it. The caller observes committed data, not pg_stat's incidental snapshots. */
export async function committedSourceVisibility<Result>(database:SqlDatabase,write:(transaction:SqlDatabase)=>Promise<unknown>,read:()=>Promise<Result>){
 if(!isTransactionalSqlDatabase(database))throw new Error('SOURCE_VISIBILITY_REQUIRES_OWNED_POOL');
 let ready:(pid:number)=>void=()=>{},failed:(cause:unknown)=>void=()=>{},release:()=>void=()=>{};
 const staged=new Promise<number>((resolve,reject)=>{ready=resolve;failed=reject;}),finish=new Promise<void>(resolve=>{release=resolve;});
 const writer=database.transaction(async transaction=>{await write(transaction);const pid=await transaction.prepare('SELECT pg_backend_pid() pid').first<number>('pid');if(!pid)throw new Error('SOURCE_WRITER_PID_REQUIRED');ready(pid);await finish;}).catch(cause=>{failed(cause);throw cause;});
 try{
  const pid=await staged;let completed=false;
  const observation=read().then(value=>({value}),cause=>({cause})).finally(()=>{completed=true;});
  let blocked=false;
  for(let attempt=0;attempt<100;attempt++){
   blocked=!!await database.prepare('SELECT EXISTS(SELECT 1 FROM pg_stat_activity activity WHERE ?::int=ANY(pg_blocking_pids(activity.pid))) blocked').bind(pid).first<boolean>('blocked');
   if(blocked||completed)break;await new Promise(resolve=>setTimeout(resolve,10));
  }
  release();await writer;const result=await observation;
  if(!blocked)throw new Error('OWNER_DID_NOT_WAIT_FOR_NATIVE_SOURCE_WRITER');
  if('cause' in result)throw result.cause;return result.value;
 }finally{release();await writer.catch(()=>{});}
}
