import {beforeAll,it,expect,vi,afterEach} from 'vitest';
import {readFile} from 'node:fs/promises';
import {randomUUID} from 'node:crypto';
import {env,applyMigrations} from '../../support/runtime-env';
import {productSsoStore} from '@erpx/server-identity_access/adapters/product-sso-store';
import {workspaceRouteCompany} from '@erpx/server-organization/public/workspace-route-company';

const control=env.CONTROL_DB,db=env.H001_DB;
vi.mock('@erpx/server-runtime/adapters/runtime',()=>({runtimeEnv:async()=>env}));
afterEach(()=>vi.unstubAllEnvs());
beforeAll(async()=>{await applyMigrations(control,env.CONTROL_TEST_MIGRATIONS);await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
it('shares one host audience across applications and refuses a different Holding and revoked parent',async()=>{
 const now=Date.now(),account='https://account-dev-erpx.jesadakorn.com',one='holding:development:h001:acme.jesadakorn.com',two='holding:development:h002:northstar.jesadakorn.com';
 const store=productSsoStore(control,'development',{account,[one]:'https://acme.jesadakorn.com',[two]:'https://northstar.jesadakorn.com'});
 const user=randomUUID(),parent=randomUUID();
 await control.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt")VALUES(?,?,?,1,?,?)').bind(user,'Holding routing fixture',`${user}@example.test`,now,now).run();
 await control.prepare('INSERT INTO session(id,token,"userId","createdAt","updatedAt","expiresAt")VALUES(?,?,?,?,?,?)').bind(parent,randomUUID(),user,new Date(now).toISOString(),new Date(now).toISOString(),new Date(now+3600000).toISOString()).run();
 await store.registerCentral({id:parent,createdAt:new Date(now),expiresAt:new Date(now+3600000)});
 const browser=await store.start(one,'/bangkok/accounting/finance?tag=one&tag=two'),grant=await store.authorize({transactionId:browser.id,state:browser.state,challenge:browser.challenge,parentSessionId:parent});
 const input={code:grant.code,state:browser.state,browserCookie:browser.browserCookie};
 await expect(store.consume({...input,audience:two})).rejects.toThrow('INVALID_SSO_TRANSACTION');
 const child=await store.consume({...input,audience:one});
 expect(child.returnPath).toBe('/bangkok/accounting/finance?tag=one&tag=two');
 expect((await store.session(child.token,one))?.user.id).toBe(user);
 for(const [key,value]of Object.entries({ERPX_HOLDING_ROUTING:'enabled',ERPX_ENVIRONMENT:'local',ERPX_PRODUCT_ENVIRONMENT:'development',ERPX_PRODUCT_ROUTING:'enabled',ERPX_PRODUCT_SSO:'enabled'}))vi.stubEnv(key,value);
 const {createAuth}=await import('@erpx/server-identity_access/adapters/auth');
 for(const app of ['finance','people']){
  vi.stubEnv('ERPX_APP_ID',app);
  const auth=await createAuth('https://acme.jesadakorn.com');
  expect((await auth.api.getSession({headers:new Headers({host:'acme.jesadakorn.com',cookie:`__Host-erpx-product=${child.token}`})}))?.user.id).toBe(user);
  expect(await auth.api.getSession({headers:new Headers({host:'northstar.jesadakorn.com',cookie:`__Host-erpx-product=${child.token}`})})).toBeNull();
 }
 expect(await store.session(child.token,two)).toBeNull();
 expect(await store.session(child.token,'finance')).toBeNull();
 await store.revokeFamily(child.sessionId);
 expect(await store.session(child.token,one)).toBeNull();
});
it('replaces conflicting slugs without changing organization identities and restores immutability',async()=>{
 const company='route-company',branch='route-branch',dept='route-dept';
 await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status,hierarchy_revision)VALUES(?,'company',?,'Route Company',NULL,?,'active',1)").bind(company,company,company).run();
 await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision)VALUES(?,'branch','finance','Route Branch',?,?,?,'active',1)").bind(branch,company,company,branch).run();
 for(const [id,slug]of [[dept,'people'],['route-existing','people-dept']])await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision)VALUES(?,'department',?,'Route Department',?,?,?,'active',1)").bind(id,slug,branch,company,branch).run();
 const sql=await readFile('migrations/postgres/holding-write/1073_holding_workspace_route_slugs.sql','utf8');
 await db.transaction(async unit=>{await unit.exec(sql);});
 expect(await db.prepare('SELECT slug,parent_id,company_id,branch_id FROM organization_nodes WHERE id=?').bind(dept).first()).toEqual({slug:'people-dept-2',parent_id:branch,company_id:company,branch_id:branch});
 expect(await workspaceRouteCompany(db,'finance-branch','people-dept-2')).toEqual({id:company,slug:company});
 expect(await workspaceRouteCompany(db,'finance-branch','missing')).toBeNull();
 expect(await workspaceRouteCompany(db,'finance-branch','people-dept-2','foreign-company')).toBeNull();
 await expect(db.prepare("UPDATE organization_nodes SET slug='changed' WHERE id=?").bind(dept).run()).rejects.toThrow('IMMUTABLE_ORGANIZATION_ROUTE');
});
