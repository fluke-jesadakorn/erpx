/** Runs retained, unchanged B0 function bodies on pinned Node; no Rust imports. */
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import ts from 'typescript';

const commit='a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const files=['packages/server-identity_access/src/domain/mfa.ts','packages/server-identity_access/src/application/mfa.ts','packages/server-identity_access/src/adapters/iam-mfa.ts','packages/server-identity_access/src/adapters/account-session-security.ts','packages/server-identity_access/src/domain/product-sso.ts'];
const sources=[];
for(const path of files){const bytes=await readFile(path);if(!bytes.equals(execFileSync('git',['show',`${commit}:${path}`])))throw Error(`ACCOUNT_SECURITY_ORACLE_SOURCE_CHANGED:${path}`);sources.push({path,sha256:createHash('sha256').update(bytes).digest('hex')});}
const node=process.env.ERPX_NODE_BIN??'node';if(execFileSync(node,['--version'],{encoding:'utf8'}).trim()!=='v26.11.1')throw Error('ACCOUNT_SECURITY_ORACLE_NODE_REQUIRED');
const directory=resolve('.scratch/dev/account-security-oracle');await mkdir(directory,{recursive:true,mode:0o700});
async function extracted(path:string,names:string[],prefix:string){
 const source=await readFile(path,'utf8'),ast=ts.createSourceFile(path,source,ts.ScriptTarget.Latest,true),selected=new Set<string>();
 const statements=ast.statements.filter(statement=>{
  const candidates=ts.isFunctionDeclaration(statement)&&statement.name?[statement.name.text]:ts.isVariableStatement(statement)?statement.declarationList.declarations.flatMap(d=>ts.isIdentifier(d.name)?[d.name.text]:[]):[];
  for(const name of candidates)if(names.includes(name))selected.add(name);
  return candidates.some(name=>names.includes(name));
 });
 if(selected.size!==names.length)throw Error('ACCOUNT_SECURITY_ORACLE_DECLARATION_MISSING');
 const destination=resolve(directory,prefix+'.ts');await writeFile(destination,statements.map(statement=>statement.getText(ast).replace(/^export\s+/, '')).join('\n')+`\nexport {${names.join(',')}};\n`,{mode:0o600});return destination;
}
const totpPath=await extracted(files[2]!,['alphabet','totp'],'totp');
await writeFile(totpPath,`import {createHmac} from 'node:crypto';\n`+await readFile(totpPath,'utf8'));
const seedPath=await extracted(files[3]!,['digest','accountKey','familyHandle','freshUntil','seed'],'seed');
await writeFile(seedPath,`import {createHash} from 'node:crypto';\nimport {bindingIsCurrent} from ${JSON.stringify(resolve(files[4]!))};\n`+await readFile(seedPath,'utf8'));
const entry=resolve(directory,'entry.ts');
await writeFile(entry,`
import {totp} from ${JSON.stringify(totpPath)};
import {seed} from ${JSON.stringify(seedPath)};
import {encodeBase32,authenticatorCodeValid,authenticatorCounters} from ${JSON.stringify(resolve(files[0]!))};
import {createBeginMfa,createVerifyMfa} from ${JSON.stringify(resolve(files[1]!))};
const now=1800000000000;Date.now=()=>now;
const actor={userId:'synthetic-user',sessionId:'synthetic-session'};
const secret=encodeBase32(new TextEncoder().encode('12345678901234567890'));
const vectors=[...Array.from({length:10},(_,counter)=>({counter})),...[59,1111111109,1111111111,1234567890,2000000000,20000000000].map(seconds=>({counter:Math.floor(seconds/30)}))].map(row=>({...row,secret,expected:totp(secret,row.counter)}));
const codes=['000000','999999','12345','1234567','１２３４５６',' 123456','12345a',''];
const rules=[];
for(const options of [{id:'previous',offset:-1},{id:'current',offset:0},{id:'next',offset:1},{id:'outside',offset:2},{id:'reused',offset:0,lastCounter:now/30000},{id:'malformed',literal:'１２３４５６'},{id:'not-enrolled',missing:true},{id:'counter-cas-failed',cas:false}]){
 const calls={throttles:0,activated:null,verified:null};const error=(status,code,message)=>Object.assign(Error(message),{status,code});
 const run=createVerifyMfa({throttle:async()=>{calls.throttles++},enrollment:async()=>options.missing?null:{secret,last_counter:options.lastCounter??-1},now:()=>now,error,open:async v=>v,matches:(secret,counter,code)=>totp(secret,counter)===code,activate:async(_actor,counter)=>{calls.activated=counter;return options.cas!==false},verified:async(_actor,at)=>{calls.verified=at}});
 let expected=null,expectedError=null;try{expected=await run(actor,options.literal??totp(secret,now/30000+(options.offset??0)))}catch(error){expectedError={code:error.code,status:error.status}}
 rules.push({id:options.id,expected,expectedError,calls});
}
const enrollments=[];
for(const userId of ["synthetic-user","ไทย é é !~*'()/:"]){
 const run=createBeginMfa({throttle:async()=>{},enabled:async()=>false,sessionCreated:async()=>new Date(now).toISOString(),now:()=>now,error:(status,code)=>Object.assign(Error(code),{status,code}),secret:()=>secret,seal:async()=> 'synthetic-envelope',enroll:async()=>{}});
 enrollments.push({userId,...await run({...actor,userId})});
}
const expires=new Date(now+43200000).toISOString(),created=new Date(now).toISOString();
const session=(id,user='synthetic-user')=>({id,user_id:user,created_at:created,expires_at:expires,user_agent:null,image:'opaque-source-session:'+id});
const binding=(id,parent=null,audience='account')=>({session_id:id,parent_session_id:parent,audience,environment:'development',authenticated_at:now,absolute_expires_at:now+43200000,image:'opaque-source-binding:'+id});
const base={sessions:[session('synthetic-session')],bindings:[binding('synthetic-session')],mfa:null,verifications:[],user:'opaque-source-user'};
const multi={...base,sessions:[session('child-a'),session('child-b'),session('foreign','foreign-user'),session('legacy'),session('other'),session('synthetic-session')],bindings:[binding('child-a','other','holding:development:h001:acme.example.test'),binding('child-b','other','holding:development:h002:northstar.example.test'),binding('foreign'),binding('other'),binding('synthetic-session')],mfa:{enabled:true,image:'opaque-source-mfa'},verifications:[{session_id:'synthetic-session',user_id:'synthetic-user',verified_at:now,image:'opaque-source-verification'}]};
const snapshots=[];
for(const [id,image]of [['single',base],['multiple-legacy-foreign',multi],['changed-encryption-image',{...multi,mfa:{enabled:true,image:'rotated-mfa-envelope'}}],['expired-mfa',{...multi,verifications:[{...multi.verifications[0],verified_at:now-300000}]}],['disabled-mfa',{...multi,mfa:{enabled:false,image:'disabled'}}],['unicode-agent',{...base,sessions:[{...base.sessions[0],user_agent:'ไทย é é 😀 punctuation ~!_'+ 'x'.repeat(600)}]}],['ordered-mfa-verifications',{...multi,verifications:[{session_id:'other',user_id:'synthetic-user',verified_at:now-1000,image:'older' },...multi.verifications]}]])snapshots.push({id,actor,image,now,expected:seed(image,actor)});
const displayBoundaries=[498,499,500].map(length=>{const image={...base,sessions:[{...base.sessions[0],user_agent:'x'.repeat(length)+'😀tail'}]},projected=seed(image,actor);return {length,actor,image,now,expectedRevision:projected.revision,expectedB0Json:JSON.stringify(projected.families[0].userAgent),reviewedNativeDisplay:'x'.repeat(length)+(length===498?'😀':'')};});
process.stdout.write(JSON.stringify({node:process.version,secret,vectors,codes:codes.map(code=>({code,expected:authenticatorCodeValid(code)})),counters:authenticatorCounters(now/30000),rules,enrollments,snapshots,displayBoundaries}));
`,{mode:0o600});
const built=await Bun.build({entrypoints:[entry],outdir:directory,target:'node',format:'esm',naming:'oracle.mjs',plugins:[{name:'actual-mfa-domain',setup(build){build.onResolve({filter:/^@erpx\/server-identity_access\/domain\/mfa$/},()=>({path:resolve(files[0]!)}));}}]});if(!built.success)throw Error('ACCOUNT_SECURITY_ORACLE_BUILD_FAILED');
const output=JSON.parse(execFileSync(node,[resolve(directory,'oracle.mjs')],{encoding:'utf8',maxBuffer:4*1024*1024,env:{PATH:process.env.PATH??'',TZ:'UTC',LANG:'en_US.UTF-8',NODE_ENV:'test'}}));
const result=JSON.stringify({version:1,sourceCommit:commit,sources,...output},null,2)+'\n',destination=resolve('fixtures/identity_access/account-security/conformance.v1.json');
if(process.argv.includes('--check')){if(await readFile(destination,'utf8')!==result)throw Error('ACCOUNT_SECURITY_ORACLE_DRIFT');}else{await mkdir(resolve(destination,'..'),{recursive:true});await writeFile(destination,result);}
console.log(`Account security Node oracle: ${output.vectors.length} TOTP, ${output.rules.length} application rules, ${output.snapshots.length} source projections.`);
