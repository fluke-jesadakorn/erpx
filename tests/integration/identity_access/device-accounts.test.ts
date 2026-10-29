import { afterAll, beforeAll, expect, it, vi } from 'vitest';
import { randomUUID } from 'node:crypto';
import { hashPassword } from 'better-auth/crypto';
import { env, applyMigrations } from '../../support/runtime-env';
import type { DeviceAccounts } from '@erpx/feature-identity_access/lib/device-accounts';

vi.mock('@erpx/server-runtime/public/runtime', () => ({ runtimeEnv: async () => env,activeHoldingBindings:async()=>[] }));
vi.mock('@erpx/server-runtime/public/runtime/env', async original => ({ ...await original<typeof import('@erpx/server-runtime/public/runtime/env')>(), getRuntime: () => env }));
const issuer = 'https://account-dev-erpx.jesadakorn.com', finance = 'https://finance-dev-erpx.jesadakorn.com';
const fixturePassword = 'Fictional-device-account-2026!';
const db = env.CONTROL_DB;
beforeAll(async () => {
  await applyMigrations(db, env.CONTROL_TEST_MIGRATIONS);
  vi.stubEnv('ERPX_ENVIRONMENT', 'local'); vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT', 'development'); vi.stubEnv('ERPX_PRODUCT_ROUTING', 'enabled'); vi.stubEnv('ERPX_PRODUCT_SSO', 'enabled');
});
afterAll(() => vi.unstubAllEnvs());
type Jar = Map<string,string>;
function absorb(jar:Jar,response:Response){for(const line of response.headers.getSetCookie()){const pair=line.split(';',1)[0],at=pair.indexOf('='),name=pair.slice(0,at),value=pair.slice(at+1);if(!value||/max-age=0(?:;|$)/i.test(line))jar.delete(name);else jar.set(name,value);}}
function request(origin:string,path:string,jar:Jar,method='GET',body?:unknown,requestOrigin=origin){return new Request(origin+path,{method,headers:{host:new URL(origin).host,origin:requestOrigin,cookie:[...jar].map(([key,value])=>`${key}=${value}`).join('; '),'content-type':'application/json'},...(body===undefined?{}:{body:JSON.stringify(body)})});}
async function account(){
  const id=randomUUID(),email=`${id}@example.test`,now=Date.now();
  await db.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,?,?)').bind(id,`Fictional device ${id}`,email,now,now).run();
  await db.prepare('INSERT INTO account(id,"accountId","providerId","userId",password,"createdAt","updatedAt") VALUES(?,?,\'credential\',?,?,?,?)').bind(randomUUID(),id,id,await hashPassword(fixturePassword),now,now).run();
  return{id,email};
}
async function normalLogin(jar:Jar,email:string){
  const {POST}=await import('@erpx/workplace/routes/api/auth/[...all]/route');
  const response=await POST(request(issuer,'/api/auth/sign-in/email',jar,'POST',{email,password:fixturePassword,callbackURL:'/'}));
  expect(response.status).toBe(200);absorb(jar,response);return response;
}
async function call(jar:Jar,action='',method='GET',body?:unknown,origin=finance){
  const {deviceAccountsRequest}=await import('@erpx/server-identity_access/public/device-accounts');
  const response=await deviceAccountsRequest(request(issuer,'/api/v1/account/device-accounts'+action,jar,method,body,origin));
  absorb(jar,response);return response;
}
async function state(jar:Jar){const response=await call(jar);expect(response.status).toBe(200);return(await response.json()).data as DeviceAccounts;}
async function child(jar:Jar,audience:'finance'|'people'){
  const {runtimeProductSsoStore}=await import('@erpx/server-identity_access/adapters/product-sso');
  const {createAuth}=await import('@erpx/server-identity_access/adapters/auth');
  const central=await(await createAuth(issuer)).api.getSession({headers:request(issuer,'/',jar).headers});expect(central).not.toBeNull();
  const store=await runtimeProductSsoStore(),transaction=await store.start(audience,'/');
  const grant=await store.authorize({transactionId:transaction.id,state:transaction.state,challenge:transaction.challenge,parentSessionId:central!.session.id});
  const issued=await store.consume({audience,code:grant.code,state:transaction.state,browserCookie:transaction.browserCookie});
  return{...issued,store,parentId:central!.session.id};
}

it('normal Account login registers a browser and the cross-origin projection exposes no bearer credentials',async()=>{
  const user=await account(),jar:Jar=new Map();await normalLogin(jar,user.email);
  const response=await call(jar),data=(await response.json()).data as DeviceAccounts;
  expect(response.headers.get('access-control-allow-origin')).toBe(finance);expect(response.headers.get('access-control-allow-credentials')).toBe('true');
  expect(data).toMatchObject({initialized:true,maximum:5,revision:1,accounts:[{email:user.email,current:true}]});
  const serialized=JSON.stringify(data);expect(serialized).not.toContain('token');expect(serialized).not.toContain('sessionId');expect(Object.keys(data.accounts[0]).sort()).toEqual(['current','email','handle','image','name']);
  expect([...jar.keys()].some(key=>key==='__Host-erpx-device')).toBe(true);
  expect((await call(jar,'','OPTIONS',undefined,'https://foreign.example.test')).status).toBe(403);
  expect((await call(jar,'','OPTIONS',undefined,'https://portal-dev-erpx.jesadakorn.com')).status).toBe(403);
  const preflight=await call(jar,'','OPTIONS');expect(preflight.status).toBe(204);expect(preflight.headers.get('access-control-allow-origin')).toBe(finance);
});
it('failed drawer sign-in preserves the selected parent, product sessions and browser revision',async()=>{
  const a=await account(),jar:Jar=new Map();await normalLogin(jar,a.email);const original=await state(jar),issued=await child(jar,'finance');
  const response=await call(jar,'/sign-in','POST',{email:a.email,password:'Wrong-fictional-password!',revision:original.revision});
  expect(response.status).toBe(401);expect(await state(jar)).toEqual(original);expect((await issued.store.session(issued.token,'finance'))?.user.id).toBe(a.id);
});
it('A to B to A invalidates every old child while preserving saved accounts and independent devices',async()=>{
  const a=await account(),b=await account(),jar:Jar=new Map(),otherDevice:Jar=new Map();
  await normalLogin(jar,a.email);await normalLogin(otherDevice,a.email);const independent=await child(otherDevice,'finance');
  const oldFinance=await child(jar,'finance'),oldPeople=await child(jar,'people'),before=await state(jar);
  const added=await call(jar,'/sign-in','POST',{email:b.email,password:fixturePassword,revision:before.revision});expect(added.status).toBe(200);
  const selected=await state(jar);expect(selected.accounts).toHaveLength(2);expect(selected.accounts.find(item=>item.current)?.email).toBe(b.email);
  expect(await oldFinance.store.session(oldFinance.token,'finance')).toBeNull();expect(await oldPeople.store.session(oldPeople.token,'people')).toBeNull();
  expect((await independent.store.session(independent.token,'finance'))?.user.id).toBe(a.id);
  expect(await db.prepare('SELECT 1 FROM session WHERE id=?').bind(oldFinance.parentId).first()).not.toBeNull();
  const switched=await call(jar,'/active','POST',{handle:selected.accounts.find(item=>item.email===a.email)!.handle,revision:selected.revision});expect(switched.status).toBe(200);
  const restored=await child(jar,'finance');expect((await restored.store.session(restored.token,'finance'))?.user.id).toBe(a.id);
  expect(await oldFinance.store.session(oldFinance.token,'finance')).toBeNull();
  const stale=await call(jar,'/active','POST',{handle:selected.accounts.find(item=>item.email===b.email)!.handle,revision:selected.revision});expect(stale.status).toBe(409);
  const current=await state(jar),removed=await call(jar,'','DELETE',{handle:current.accounts.find(item=>item.email===a.email)!.handle,revision:current.revision});expect(removed.status).toBe(200);
  expect((await state(jar)).accounts).toMatchObject([{email:b.email,current:false}]);expect(await restored.store.session(restored.token,'finance')).toBeNull();
  expect((await independent.store.session(independent.token,'finance'))?.user.id).toBe(a.id);
});
it('rejects a sixth account before authentication and allows reauthentication of an existing account at the limit',async()=>{
  const users=await Promise.all(Array.from({length:6},account)),jar:Jar=new Map();await normalLogin(jar,users[0].email);
  for(const user of users.slice(1,5)){const before=await state(jar);expect((await call(jar,'/sign-in','POST',{email:user.email,password:fixturePassword,revision:before.revision})).status).toBe(200);}
  const full=await state(jar),response=await call(jar,'/sign-in','POST',{email:users[5].email,password:fixturePassword,revision:full.revision});expect(response.status).toBe(409);
  expect((await response.json()).error.code).toBe('DEVICE_ACCOUNT_LIMIT');expect(await state(jar)).toEqual(full);
  expect(await db.prepare('SELECT 1 FROM session WHERE "userId"=?').bind(users[5].id).first()).toBeNull();
  expect((await call(jar,'/sign-in','POST',{email:users[0].email,password:fixturePassword,revision:full.revision})).status).toBe(200);
  expect((await state(jar)).accounts).toHaveLength(5);expect((await state(jar)).accounts.find(item=>item.current)?.email).toBe(users[0].email);
});
it('expires saved accounts, rejects forged handles and never accepts another browsers saved account',async()=>{
  const a=await account(),b=await account(),jar:Jar=new Map(),other:Jar=new Map();await normalLogin(jar,a.email);await normalLogin(other,b.email);
  const own=await state(jar),foreign=await state(other);
  expect((await call(jar,'/active','POST',{handle:foreign.accounts[0].handle,revision:own.revision})).status).toBe(401);
  expect((await call(jar,'/active','POST',{handle:'x'.repeat(32),revision:own.revision})).status).toBe(401);
  const issued=await child(jar,'people');await db.prepare('UPDATE session SET "expiresAt"=? WHERE id=?').bind(new Date(Date.now()-1000).toISOString(),issued.parentId).run();
  expect((await state(jar)).accounts).toEqual([]);expect(await issued.store.session(issued.token,'people')).toBeNull();
});
it('normal auth callback synchronization selects the same browser and rejects a sixth parent without changing the selected account',async()=>{
 const jar:Jar=new Map(),users=await Promise.all(Array.from({length:6},()=>account()));await normalLogin(jar,users[0].email);
 const {createAuth}=await import('@erpx/server-identity_access/adapters/auth'),auth=await createAuth(issuer),{synchronizeAccountAuth,accountAuthenticationRequest}=await import('@erpx/server-identity_access/public/device-accounts');
 const callback=async(email:string)=>{
  const incoming=request(issuer,'/api/auth/sign-in/email',jar,'POST',{email,password:fixturePassword}),raw=await auth.handler(await accountAuthenticationRequest(incoming));expect(raw.status).toBe(200);
  return synchronizeAccountAuth(incoming,raw);
 };
 absorb(jar,await callback(users[1].email));expect((await state(jar)).accounts.find(account=>account.current)?.email).toBe(users[1].email);
 for(const user of users.slice(2,5))await normalLogin(jar,user.email);
 const before=await state(jar),issued=await child(jar,'finance');await expect(callback(users[5].email)).rejects.toMatchObject({code:'DEVICE_ACCOUNT_LIMIT'});
 expect(await state(jar)).toEqual(before);expect((await issued.store.session(issued.token,'finance'))?.user.id).toBe(users[4].id);expect(await db.prepare('SELECT 1 FROM session WHERE "userId"=?').bind(users[5].id).first()).toBeNull();
});
it('removal and revoked parents free a device-cookie slot so a replacement stays available after switching away',async()=>{
 const jar:Jar=new Map(),users=await Promise.all(Array.from({length:7},()=>account()));for(const user of users.slice(0,5))await normalLogin(jar,user.email);
 let before=await state(jar);const removed=before.accounts.find(account=>account.email===users[0].email)!;
 expect((await call(jar,'','DELETE',{handle:removed.handle,revision:before.revision})).status).toBe(200);
 expect((await call(jar,'/sign-in','POST',{email:users[5].email,password:fixturePassword,revision:(await state(jar)).revision})).status).toBe(200);
 for(const user of [users[4],users[5]]){before=await state(jar);expect((await call(jar,'/active','POST',{handle:before.accounts.find(account=>account.email===user.email)!.handle,revision:before.revision})).status).toBe(200);}
 await db.prepare('DELETE FROM session WHERE "userId"=?').bind(users[1].id).run();
 await normalLogin(jar,users[6].email);before=await state(jar);expect(before.accounts).toHaveLength(5);
 expect((await call(jar,'/active','POST',{handle:before.accounts.find(account=>account.email===users[5].email)!.handle,revision:before.revision})).status).toBe(200);
 before=await state(jar);expect((await call(jar,'/active','POST',{handle:before.accounts.find(account=>account.email===users[6].email)!.handle,revision:before.revision})).status).toBe(200);
 expect((await state(jar)).accounts.find(account=>account.current)?.email).toBe(users[6].email);
});
it('serializes conflicting tab selections and rejects a completed login after its reservation expired',async()=>{
  const a=await account(),b=await account(),jar:Jar=new Map();await normalLogin(jar,a.email);
  expect((await call(jar,'/sign-in','POST',{email:b.email,password:fixturePassword,revision:(await state(jar)).revision})).status).toBe(200);
  const before=await state(jar),first=before.accounts.find(item=>item.email===a.email)!;
  const responses=await Promise.all([call(new Map(jar),'/active','POST',{handle:first.handle,revision:before.revision}),call(new Map(jar),'/active','POST',{handle:first.handle,revision:before.revision})]);
  expect(responses.map(response=>response.status).sort()).toEqual([200,409]);
  const {deviceAccountStore,deviceBrowserCodec}=await import('@erpx/server-identity_access/adapters/device-account-store');
  const {createAuth}=await import('@erpx/server-identity_access/adapters/auth'),auth=await createAuth(issuer),devices=await auth.api.listDeviceSessions({headers:request(issuer,'/',jar).headers});
  const id=deviceBrowserCodec(env.BETTER_AUTH_SECRET).decode(decodeURIComponent(jar.get('__Host-erpx-device')!))!,proof=devices.map(item=>item.session.id);
  let instant=Date.now();const store=deviceAccountStore(db,id,()=>instant),reservation=await store.reserve(proof,a.email);instant+=120001;
  await expect(store.complete(proof,proof[0],reservation.nonce)).rejects.toMatchObject({code:'DEVICE_ACCOUNTS_CHANGED'});
  await store.release(reservation.nonce);
});

it('switching waits for the native session-first command lease and rejects the old actor after committing',async()=>{
 const a=await account(),b=await account(),jar:Jar=new Map();await normalLogin(jar,a.email);
 expect((await call(jar,'/sign-in','POST',{email:b.email,password:fixturePassword,revision:(await state(jar)).revision})).status).toBe(200);
 const issued=await child(jar,'finance'),before=await state(jar),handle=before.accounts.find(item=>item.email===a.email)!.handle;
 const {withCurrentWorkSelectionSession}=await import('@erpx/server-identity_access/public/private-session-read');
 let release:()=>void=()=>{},entered:()=>void=()=>{};
 const held=new Promise<void>(resolve=>{release=resolve;}),ready=new Promise<void>(resolve=>{entered=resolve;});
 const actor={userId:b.id,sessionId:issued.sessionId,email:b.email,audience:'finance'};
 const command=withCurrentWorkSelectionSession(actor,async assertCurrent=>{entered();await held;await assertCurrent();return 'committed-under-original-account';});
 await ready;let switched=false;const switching=call(jar,'/active','POST',{handle,revision:before.revision}).then(response=>{switched=true;return response;});
 try{
  let waiting=false;for(let attempt=0;attempt<100&&!switched;attempt++){waiting=!!await db.prepare("SELECT 1 FROM pg_locks WHERE relation='session'::regclass AND NOT granted LIMIT 1").first();if(waiting)break;await new Promise(resolve=>setTimeout(resolve,10));}
  expect({waiting,switched}).toEqual({waiting:true,switched:false});
 }finally{release();}
 expect(await command).toBe('committed-under-original-account');expect((await switching).status).toBe(200);
 await expect(withCurrentWorkSelectionSession(actor,async()=> 'must not run')).rejects.toMatchObject({code:'SESSION_REVOKED'});
 expect(await issued.store.session(issued.token,'finance')).toBeNull();
});
