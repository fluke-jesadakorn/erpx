import {randomUUID} from 'node:crypto';
import {hashPassword} from 'better-auth/crypto';
import {afterAll,beforeAll,expect,it,vi} from 'vitest';
import {env,applyMigrations} from '../../support/runtime-env';
import {seedExpenseCompany} from '../../../fixtures/expenses/company';
const requestContext=vi.hoisted(()=>({headers:new Headers()}));
vi.mock('next/headers',()=>({headers:async()=>requestContext.headers}));
vi.mock('@/server/modules/runtime/adapters/runtime',async original=>({...await original<typeof import('@/server/modules/runtime/adapters/runtime')>(),runtimeEnv:async()=>env,holdingBindingBySlug:async(slug:string)=>{if(slug!=='test')throw Error('Original test Holding required');return {id:'h001',slug:'test',displayName:'Test'};},holdingResources:async(id:string)=>{if(id!=='h001')throw Error('Original test Holding required');return {write:env.H001_DB,files:env.H001_FILES};}}));
import {POST} from '@/app/api/legal-portal/[holdingSlug]/[[...path]]/route';
import {createAuth} from '@/server/modules/identity_access/public/auth';
import {productSsoRequest} from '@/server/modules/identity_access/public/product-sso';
import {legalCommand,detail} from '@/server/modules/legal/public/legal/service';
import {access} from '@/server/modules/legal/public/legal/access';
import {retentionCommand,retentionView} from '@/server/modules/legal/public/legal/retention';
import {measureReads} from '@/server/modules/runtime/public/runtime/read-metrics';
const holding=env.H001_DB,control=env.CONTROL_DB,issuer='https://account-dev-erpx.jesadakorn.com',portal='https://legal-portal-dev-erpx.jesadakorn.com';
beforeAll(async()=>{await applyMigrations(control,env.CONTROL_TEST_MIGRATIONS);await applyMigrations(holding,env.STAFF_TEST_MIGRATIONS);vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');vi.stubEnv('ERPX_LEGAL_PACKAGE_ENFORCEMENT','disabled');});
afterAll(()=>vi.unstubAllEnvs());
function request(origin:string,path:string,cookie='',method='GET',body?:unknown){return new Request(origin+path,{method,headers:{host:new URL(origin).host,origin,cookie,'content-type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});}
const cookies=(response:Response)=>response.headers.getSetCookie().map(value=>value.split(';')[0]).join('; ');
async function normalPortalAccount(){
 const userId=randomUUID(),email=userId+'@example.test',now=Date.now(),password='Fictional-Inspection-Ordering-2026!';await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(userId,'Original external document reviewer',email,now,now).run();await control.prepare('INSERT INTO account(id,"accountId","providerId","userId",password,"createdAt","updatedAt") VALUES(?,?,\'credential\',?,?,?,?)').bind(randomUUID(),userId,userId,await hashPassword(password),now,now).run();
 const signedIn=await(await createAuth(issuer)).handler(request(issuer,'/api/auth/sign-in/email','','POST',{email,password}));expect(signedIn.status).toBe(200);const central=cookies(signedIn),start=await productSsoRequest(request(portal,'/sso/start?next='+encodeURIComponent('/legal-portal/test')),'start');expect(start.status).toBe(303);const authorizeUrl=new URL(start.headers.get('location')!),authorized=await productSsoRequest(request(issuer,authorizeUrl.pathname+authorizeUrl.search,central),'authorize');expect(authorized.status).toBe(303);const callbackUrl=new URL(authorized.headers.get('location')!),callback=await productSsoRequest(request(portal,callbackUrl.pathname+callbackUrl.search,cookies(start)),'callback');expect(callback.status).toBe(303);const cookie=cookies(callback);expect(await holding.prepare('SELECT 1 FROM assignments WHERE user_id=?').bind(userId).first()).toBeNull();expect(await control.prepare('SELECT 1 FROM platform_assignments WHERE user_id=?').bind(userId).first()).toBeNull();return {userId,email,cookie};
}

it('orders a normal disabled retention assignment before its waiting normal Portal comment without a deadlock or hidden retry',async()=>{
 const measured=await measureReads(async()=>{
  const account=await normalPortalAccount(),f=await seedExpenseCompany(holding,env.H001_FILES),owner=await f.member('original-retention-manager','legal_manager'),reviewer=await f.member('independent-retention-reviewer','legal_manager');
  await legalCommand(holding,owner,{action:'settings',enabled:true},randomUUID());
  const item=await legalCommand(holding,owner,{action:'create',item:{kind:'contract',classification:'confidential',title:'Original retained contract',ownerId:owner.userId,data:{contractType:'Synthetic',parties:'Company and external reviewer'}}},randomUUID());
  const command=async(action:string,extra:Record<string,unknown>={})=>legalCommand(holding,owner,{action,id:item.id,expectedVersion:(await access(holding,owner,item.id)).version,...extra},randomUUID());
  await command('document',{name:'Retained original evidence',body:'Original retained evidence, no physical destruction.'});
  const document=(await detail(holding,owner,item.id)).documents[0];
  await command('share',{documentId:document.id,email:account.email,expiresAt:Date.now()+86400000});
  const share=(await detail(holding,owner,item.id)).shares[0];
  await command('transition',{state:'cancelled',reason:'Original contract cancellation'});
  await command('transition',{state:'archived',reason:'Original retained contract archive'});
  const policyId=randomUUID();
  await retentionCommand(holding,owner,{action:'retention.policy',name:'Original one day policy',days:1,reason:'Native retained contract policy'},policyId);
  await retentionCommand(holding,reviewer,{action:'retention.approve-policy',policyId,policyVersion:1,reason:'Independent native retention policy review'},randomUUID());
  const version=(await access(holding,owner,item.id)).version,writerKey=randomUUID(),commentKey=randomUUID(),name='legal_retention_order_'+randomUUID().replaceAll('-',''),gate=await holding.pool.connect(),gatePid=Number((await gate.query<{pid:number}>('SELECT pg_backend_pid() pid')).rows[0].pid);
  await gate.query('SELECT pg_advisory_lock(hashtextextended($1,0))',[name]);
  await holding.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${writerKey}' AND NEW.step='legal-content-available' THEN PERFORM pg_advisory_xact_lock(hashtextextended('${name}',0));END IF;RETURN NEW;END $$;CREATE TRIGGER aa_${name} INSTEAD OF INSERT ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${name}()`);
  const input={action:'retention.assign',id:item.id,expectedVersion:version,policyId,reason:'Assign independently approved original policy'},writing=retentionCommand(holding,owner,input,writerKey);let external:Promise<Response>|undefined,writerPid:number|undefined;
  try{
   for(let i=0;i<100;i++){const row=await holding.prepare('SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND ?=ANY(pg_blocking_pids(pid)) LIMIT 1').bind(gatePid).first<{pid:number}>();if(row){writerPid=Number(row.pid);break;}await new Promise(resolve=>setTimeout(resolve,10));}
   expect(writerPid).toBeDefined();
   const req=request(portal,'/api/legal-portal/test',account.cookie,'POST',{action:'comment',shareId:share.id,body:'Original external review after retention assignment'});req.headers.set('Idempotency-Key',commentKey);requestContext.headers=req.headers;external=POST(req,{params:Promise.resolve({holdingSlug:'test',path:[]})});
   let waiting=false;for(let i=0;i<100;i++){waiting=!!await holding.prepare('SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND ?=ANY(pg_blocking_pids(pid)) LIMIT 1').bind(writerPid!).first();if(waiting)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(waiting).toBe(true);
  }finally{
   await gate.query('SELECT pg_advisory_unlock(hashtextextended($1,0))',[name]);await Promise.allSettled([writing,...(external?[external]:[])]);gate.release();await holding.exec(`DROP TRIGGER aa_${name} ON command_assertions;DROP FUNCTION ${name}()`);
  }
  expect(await writing).toMatchObject({id:item.id,version:version+1});const response=await external!;expect(response.status).toBe(200);expect(await response.json()).toMatchObject({data:{id:share.id}});
  expect((await retentionView(holding,owner,item.id)).assignment).toMatchObject({item_id:item.id,policy_id:policyId,policy_version:2});
  expect((await access(holding,owner,item.id)).version).toBe(version+1);
  expect(await retentionCommand(holding,owner,input,writerKey)).toEqual(await writing);
  expect(await holding.prepare("SELECT count(*)::int n FROM legal_events WHERE item_id=? AND action='retention.assign'").bind(item.id).first('n')).toBe(1);
  expect(await holding.prepare('SELECT count(*)::int n FROM legal_comments WHERE created_by=?').bind(account.userId).first('n')).toBe(1);
  expect(await holding.prepare('SELECT count(*)::int n FROM legal_portal_receipts WHERE id=?').bind(commentKey).first('n')).toBe(1);
 });
 expect({deadlocks:measured.metrics.deadlockFailures??0,serialization:measured.metrics.serializationFailures??0,retries:measured.metrics.transactionRetries??0}).toEqual({deadlocks:0,serialization:0,retries:0});
});
