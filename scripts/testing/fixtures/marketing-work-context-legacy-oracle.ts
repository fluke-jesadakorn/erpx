/** Temporary mapper differential, removed under retire.marketing.asset-writers. */
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {createNativeWorkRoleTransport} from '@erpx/server-identity_access/public/native-work-role-transport';
import {nativeAccountReadModel,nativeWorkReadModel} from '../../../packages/server-identity_access/src/adapters/native-account-context';
async function main(){
 if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');
 const input=JSON.parse(readFileSync(0,'utf8')) as {response:unknown;identity:{sessionId:string;userId:string;companyId:string};user:{name:string;email:string}};
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingId:'h001',holdingSlug:'acme'}]};
 const h=new Headers({host:'acme.example.test'});
 try {
  // This callback only transports the actual in-process Rust HTTP response; it
  // does not authenticate or manufacture a WorkContext.
  const transport=createNativeWorkRoleTransport(JSON.stringify(config),async()=>({status:200,body:input.response}) as never);
  const state=await transport(h,input.identity,'context',undefined,'h001');
  const session={session:{id:input.identity.sessionId},user:{id:input.identity.userId,name:input.user.name,email:input.user.email}};
  const access=nativeAccountReadModel(state,session as never,h,{kind:'holding',holdingSlug:'acme'});
  const work=nativeWorkReadModel(state,access);
  const fingerprint={actor:work.userId,assignment:work.assignmentId,role:work.role,authority:work.authority,company:work.companyId,branch:work.branchId,department:work.departmentId,operation:"marketing.asset.create",recordId:"marketing-content-fixture",input:{kind:"content",name:"Draft",configuration:{},preview:{}}};
  process.stdout.write(JSON.stringify({ok:true,role:work.role,revision:work.authority?.revision,assignmentVersion:work.authority?.assignmentVersion,hash:createHash("sha256").update(JSON.stringify(fingerprint)).digest("hex")}));
 }catch(error){const e=error as {code?:string;status?:number};process.stdout.write(JSON.stringify({ok:false,code:e.code??null,status:e.status??null}));}
}
main().catch(()=>{console.error('MARKETING_WORK_CONTEXT_ORACLE_FAILED');process.exitCode=1;});
