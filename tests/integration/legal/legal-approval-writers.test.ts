import {randomUUID} from 'node:crypto';
import {hashPassword} from 'better-auth/crypto';
import {afterAll,beforeAll,expect,it,vi} from 'vitest';
import {env,applyMigrations} from '../../support/runtime-env';
import {seedExpenseCompany} from '../../../fixtures/expenses/company';
const requestContext=vi.hoisted(()=>({headers:new Headers()}));
vi.mock('next/headers',()=>({headers:async()=>requestContext.headers}));
vi.mock("@erpx/server-runtime/adapters/runtime",async original=>({...await original<typeof import("@erpx/server-runtime/adapters/runtime")>(),runtimeEnv:async()=>env,holdingBindingBySlug:async(slug:string)=>{if(slug!=='test')throw Error('Original test Holding required');return {id:'h001',slug:'test',displayName:'Test'};},holdingResources:async(id:string)=>{if(id!=='h001')throw Error('Original test Holding required');return {write:env.H001_DB,files:env.H001_FILES};}}));
import {POST} from '@erpx/feature-legal-portal/routes/api/legal-portal/[holdingSlug]/[[...path]]/route';
import {createAuth} from '@erpx/server-identity_access/public/auth';
import {productSsoRequest} from '@erpx/server-identity_access/public/product-sso';
import {legalCommand,detail} from '@erpx/server-legal/public/legal/service';
import {access} from '@erpx/server-legal/public/legal/access';
import {legalApprovalCommand,legalApprovalView} from '@erpx/server-legal/public/legal/approval';
import {measureReads} from '@erpx/server-runtime/public/runtime/read-metrics';
const holding=env.H001_DB,control=env.CONTROL_DB,issuer='https://account-dev-erpx.jesadakorn.com',portal='https://legal-portal-dev-erpx.jesadakorn.com';
beforeAll(async()=>{await applyMigrations(control,env.CONTROL_TEST_MIGRATIONS);await applyMigrations(holding,env.STAFF_TEST_MIGRATIONS);vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');vi.stubEnv('ERPX_LEGAL_PACKAGE_ENFORCEMENT','disabled');});
afterAll(()=>vi.unstubAllEnvs());
function request(origin:string,path:string,cookie='',method='GET',body?:unknown){return new Request(origin+path,{method,headers:{host:new URL(origin).host,origin,cookie,'content-type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});}
const cookies=(response:Response)=>response.headers.getSetCookie().map(value=>value.split(';')[0]).join('; ');
async function normalPortalAccount(){
 const userId=randomUUID(),email=userId+'@example.test',now=Date.now(),password='Fictional-Inspection-Ordering-2026!';await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(userId,'Original external document reviewer',email,now,now).run();await control.prepare('INSERT INTO account(id,"accountId","providerId","userId",password,"createdAt","updatedAt") VALUES(?,?,\'credential\',?,?,?,?)').bind(randomUUID(),userId,userId,await hashPassword(password),now,now).run();
 const signedIn=await(await createAuth(issuer)).handler(request(issuer,'/api/auth/sign-in/email','','POST',{email,password}));expect(signedIn.status).toBe(200);const central=cookies(signedIn),start=await productSsoRequest(request(portal,'/sso/start?next='+encodeURIComponent('/legal-portal/test')),'start');expect(start.status).toBe(303);const authorizeUrl=new URL(start.headers.get('location')!),authorized=await productSsoRequest(request(issuer,authorizeUrl.pathname+authorizeUrl.search,central),'authorize');expect(authorized.status).toBe(303);const callbackUrl=new URL(authorized.headers.get('location')!),callback=await productSsoRequest(request(portal,callbackUrl.pathname+callbackUrl.search,cookies(start)),'callback');expect(callback.status).toBe(303);const cookie=cookies(callback);expect(await holding.prepare('SELECT 1 FROM assignments WHERE user_id=?').bind(userId).first()).toBeNull();expect(await control.prepare('SELECT 1 FROM platform_assignments WHERE user_id=?').bind(userId).first()).toBeNull();return {userId,email,cookie};
}

it('orders a normal disabled approval opening before its waiting normal Portal comment without a deadlock or hidden retry',async()=>{
 const measured=await measureReads(async()=>{
  const account=await normalPortalAccount(),f=await seedExpenseCompany(holding,env.H001_FILES),owner=await f.member('original-approval-manager','legal_manager'),reviewer=await f.member('independent-approval-reviewer','legal_counsel');
  await legalCommand(holding,owner,{action:'settings',enabled:true},randomUUID());
  const item=await legalCommand(holding,owner,{action:'create',item:{kind:'contract',classification:'confidential',title:'Original contract approval',ownerId:owner.userId,data:{contractType:'Synthetic',parties:'Company and external reviewer'}}},randomUUID());
  const command=async(action:string,extra:Record<string,unknown>={})=>legalCommand(holding,owner,{action,id:item.id,expectedVersion:(await access(holding,owner,item.id)).version,...extra},randomUUID());
  await command('document',{name:'Original reviewable evidence',body:'Original approval evidence with named signers.'});
  await command('member',{userId:reviewer.userId,access:'read'});
  const document=(await detail(holding,owner,item.id)).documents[0];
  await command('share',{documentId:document.id,email:account.email,expiresAt:Date.now()+86400000});
  const share=(await detail(holding,owner,item.id)).shares[0],version=(await access(holding,owner,item.id)).version,writerKey=randomUUID(),commentKey=randomUUID(),name='legal_approval_order_'+randomUUID().replaceAll('-',''),gate=await holding.pool.connect(),gatePid=Number((await gate.query<{pid:number}>('SELECT pg_backend_pid() pid')).rows[0].pid);
  await gate.query('SELECT pg_advisory_lock(hashtextextended($1,0))',[name]);
  await holding.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${writerKey}' AND NEW.step='legal-content-available' THEN PERFORM pg_advisory_xact_lock(hashtextextended('${name}',0));END IF;RETURN NEW;END $$;CREATE TRIGGER aa_${name} INSTEAD OF INSERT ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${name}()`);
  const input={action:'approval.open',id:item.id,expectedVersion:version,documentId:document.id,signerIds:[owner.userId],approverIds:[reviewer.userId],reason:'Independently review the exact contract original'},writing=legalApprovalCommand(holding,owner,input,writerKey);let external:Promise<Response>|undefined,writerPid:number|undefined;
  try{
   for(let i=0;i<100;i++){const row=await holding.prepare('SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND ?=ANY(pg_blocking_pids(pid)) LIMIT 1').bind(gatePid).first<{pid:number}>();if(row){writerPid=Number(row.pid);break;}await new Promise(resolve=>setTimeout(resolve,10));}
   expect(writerPid).toBeDefined();
   const req=request(portal,'/api/legal-portal/test',account.cookie,'POST',{action:'comment',shareId:share.id,body:'Original external review after approval opening'});req.headers.set('Idempotency-Key',commentKey);requestContext.headers=req.headers;external=POST(req,{params:Promise.resolve({holdingSlug:'test',path:[]})});
   let waiting=false;for(let i=0;i<100;i++){waiting=!!await holding.prepare('SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND ?=ANY(pg_blocking_pids(pid)) LIMIT 1').bind(writerPid!).first();if(waiting)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(waiting).toBe(true);
  }finally{
   await gate.query('SELECT pg_advisory_unlock(hashtextextended($1,0))',[name]);await Promise.allSettled([writing,...(external?[external]:[])]);gate.release();await holding.exec(`DROP TRIGGER aa_${name} ON command_assertions;DROP FUNCTION ${name}()`);
  }
  expect(await writing).toMatchObject({id:item.id,version:version+1,state:'approval'});const response=await external!;expect(response.status).toBe(200);expect(await response.json()).toMatchObject({data:{id:share.id}});
  expect((await legalApprovalView(holding,reviewer,item.id)).instance?.state).toBe('pending');
  expect(await holding.prepare('SELECT locked FROM legal_documents WHERE id=?').bind(document.id).first('locked')).toBe(true);
  expect(await legalApprovalCommand(holding,owner,input,writerKey)).toEqual(await writing);
  await expect(legalApprovalCommand(holding,owner,{...input,reason:'Changed captured review reason'},writerKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  expect(await holding.prepare("SELECT count(*)::int n FROM legal_events WHERE item_id=? AND action='approval.open'").bind(item.id).first('n')).toBe(1);
  expect(await holding.prepare('SELECT count(*)::int n FROM legal_comments WHERE created_by=?').bind(account.userId).first('n')).toBe(1);
  expect(await holding.prepare('SELECT count(*)::int n FROM legal_portal_receipts WHERE id=?').bind(commentKey).first('n')).toBe(1);
  await legalApprovalCommand(holding,reviewer,{action:'approval.decide',id:item.id,expectedVersion:(await access(holding,reviewer,item.id)).version,decision:'revise',reason:'Independent reviewer returns exact original for revision'},randomUUID());
  expect((await legalApprovalView(holding,reviewer,item.id)).instance?.state).toBe('cancelled');expect((await access(holding,owner,item.id)).state).toBe('draft');
  await legalApprovalCommand(holding,owner,{...input,expectedVersion:(await access(holding,owner,item.id)).version},randomUUID());
  await legalApprovalCommand(holding,reviewer,{action:'approval.decide',id:item.id,expectedVersion:(await access(holding,reviewer,item.id)).version,decision:'approve',reason:'Independent reviewer confirms the retained exact original'},randomUUID());
  expect((await legalApprovalView(holding,reviewer,item.id)).instance?.state).toBe('approved');expect((await access(holding,owner,item.id)).state).toBe('approved');
 });
 expect({deadlocks:measured.metrics.deadlockFailures??0,serialization:measured.metrics.serializationFailures??0,retries:measured.metrics.transactionRetries??0}).toEqual({deadlocks:0,serialization:0,retries:0});
});
