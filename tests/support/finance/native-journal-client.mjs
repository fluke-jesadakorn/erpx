// Test-only pinned Node caller of the actual server-only Finance adapter.
import {createInterface} from 'node:readline';import{pathToFileURL}from'node:url';
const input=createInterface({input:process.stdin,crlfDelay:Infinity});const lines=input[Symbol.asyncIterator]();const emit=v=>process.stdout.write(JSON.stringify(v)+'\n');
async function next(){const line=await lines.next();if(line.done||Buffer.byteLength(line.value)>32768)throw Error('INVALID_TEST_PROTOCOL');return JSON.parse(line.value);}
try{
 if(process.version!=='v26.11.1')throw Error('NODE_26_11_1_REQUIRED');
 const p=await next();const{createNativeJournalDetailClient}=await import(pathToFileURL(process.argv[2]).href);const read=createNativeJournalDetailClient(p.configuration);const header=cookie=>new Headers({host:'acme.example.test',cookie});
 const original=await read(header(p.cookie),p.input);if(original.context.userId!=='fixture-b'||original.data.journal.id!==p.input.journalId||original.context.assignmentVersion!==3||original.context.revision!==7)throw Error('ORIGINAL_READ_CONTEXT_MISMATCH');
 let stale=false;try{await read(header(p.cookie),{...p.input,expected:{companyId:'company',assignmentId:'finance-fixture-b',revision:7,assignmentVersion:2}});}catch(e){stale=e.code==='NATIVE_FINANCE_UNAVAILABLE'&&e.status===409;}if(!stale)throw Error('STALE_CONTEXT_ACCEPTED');
 emit({type:'read-complete'});const updated=await next();
 let retired=false;try{await read(header(p.cookie),p.input);}catch(e){retired=e.code==='NATIVE_FINANCE_UNAVAILABLE'&&e.status===401;}if(!retired)throw Error('OLD_ACCOUNT_CONTEXT_REUSED');
 const fresh=await read(header(updated.cookie),p.input);if(fresh.context.userId!=='fixture-a'||fresh.context.assignmentId!=='finance-fixture-a'||fresh.data.journal.id!==p.input.journalId||original.context.userId!=='fixture-b')throw Error('CROSS_ACCOUNT_CONTEXT_MISMATCH');
 emit({type:'done',originalPreserved:true});
}catch(error){emit({type:'failure',code:typeof error.code==='string'&&/^[A-Z0-9_]{1,80}$/.test(error.code)?error.code:typeof error.message==='string'&&/^[A-Z0-9_]{1,80}$/.test(error.message)?error.message:'NATIVE_FINANCE_CLIENT_FAILED'});process.exitCode=1;}finally{input.close();process.stdin.pause();}
