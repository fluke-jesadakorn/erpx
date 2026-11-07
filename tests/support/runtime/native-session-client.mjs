// Test-only IPC caller of the actual typed frontend adapter. No DB connection,
// fallback session, Actor or operator environment is available in this process.
import {createInterface} from 'node:readline';
import {get} from 'node:http';
import {pathToFileURL} from 'node:url';
const lines=createInterface({input:process.stdin,crlfDelay:Infinity});const iterator=lines[Symbol.asyncIterator]();
const emit=value=>process.stdout.write(JSON.stringify(value)+'\n');
async function next(expected){const row=await iterator.next();if(row.done||Buffer.byteLength(row.value)>32768||expected&&row.value!==expected)throw Error('INVALID_TEST_PROTOCOL');return row.value;}
async function control(url){await new Promise((resolve,reject)=>{const request=get(url,response=>{let data='';response.on('data',chunk=>{data+=chunk;if(data.length>64)request.destroy(Error('PROXY_CONTROL_INVALID'));});response.once('end',()=>response.statusCode===200&&data==='proxy-control-ok'?resolve():reject(Error('PROXY_CONTROL_INVALID')));});request.once('error',reject);});}
try {
 if(process.version!=='v26.11.1')throw Error('NODE_26_11_1_REQUIRED');
 const payload=JSON.parse(await next());
 const {createNativeSessionClient}=await import(pathToFileURL(process.argv[2]).href);
 // This no-cookie request proves the ambient proxy trap is actually active.
 await control(new URL('/__proxy_control',payload.configuration.privateBaseUrl));
 const read=createNativeSessionClient(payload.configuration);
 const header=row=>new Headers({host:row.host,...row.cookie?{cookie:row.cookie}:{}});
 const account=payload.cases[0],first=payload.cases[1],second=payload.cases[2];
 for(const row of payload.cases){const value=await read(header(row));if(value?.user.id!==row.expectedUser||row.expectedSession&&value?.session.id!==row.expectedSession)throw Error('SESSION_IDENTITY_MISMATCH');}
 if(await read(new Headers({host:account.host}))!==null)throw Error('ANONYMOUS_NOT_NULL');
 if(await read(new Headers({host:second.host,cookie:first.cookie}))!==null)throw Error('CROSS_HOST_COOKIE_ACCEPTED');
 emit({type:'first-complete',users:payload.cases.map(row=>row.expectedUser)});await next('withdraw');
 let denied=false;try{await read(header(first));}catch(error){denied=error.code==='NATIVE_SESSION_UNAVAILABLE'&&error.status===404;}if(!denied)throw Error('STALE_HOLDING_SESSION_REUSED');
 if((await read(header(account)))?.user.id!==account.expectedUser||(await read(header(second)))?.user.id!==second.expectedUser)throw Error('SIBLING_SESSION_ISOLATION_FAILED');
 emit({type:'ready-for-abort'});await next('blocked');
 const bounded=createNativeSessionClient({...payload.configuration,timeoutMs:250});
 const caller=new AbortController();const cancelling=bounded(header(account),caller.signal);setTimeout(()=>caller.abort(),20);
 let callerAborted=false;try{await cancelling;}catch(error){callerAborted=error.code==='NATIVE_SESSION_ABORTED';}if(!callerAborted)throw Error('CALLER_ABORT_FAILED');
 let timedOut=false;try{await bounded(header(account));}catch(error){timedOut=error.code==='NATIVE_SESSION_ABORTED';}if(!timedOut)throw Error('ABSOLUTE_DEADLINE_FAILED');
 emit({type:'abort-complete'});await next('released');
 if((await read(header(account)))?.user.id!==account.expectedUser)throw Error('FRESH_READ_AFTER_ABORT_FAILED');
 emit({type:'done',freshAfterAbort:true});
}catch(error){emit({type:'failure',code:typeof error.code==='string'&&/^[A-Z0-9_]{1,80}$/.test(error.code)?error.code:typeof error.message==='string'&&/^[A-Z0-9_]{1,80}$/.test(error.message)?error.message:'NATIVE_SESSION_CLIENT_FAILED'});process.exitCode=1;}
finally{lines.close();process.stdin.pause();}
