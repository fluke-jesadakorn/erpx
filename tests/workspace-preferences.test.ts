import {beforeAll,expect,it} from 'vitest';
import {env,applyMigrations} from './runtime-env';
import {readWorkspacePreference,readCompanyPreference,rememberWorkspace,rememberPlatformRole} from '../src/server/workspace-preferences';
beforeAll(()=>applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS));
it('stores workspace and company by account independently of browser/session',async()=>{
 const db=env.CONTROL_DB,user=crypto.randomUUID(),other=crypto.randomUUID();
 await rememberWorkspace(db,user,'/holdings/acme/home',{holdingId:'h001',companyId:'beacon-co'});
 await rememberWorkspace(db,user,'/holdings/northstar/home',{holdingId:'h002',companyId:'northstar-co'});
 expect((await readWorkspacePreference(db,user))?.workspace_path).toBe('/holdings/northstar/home');
 expect((await readCompanyPreference(db,user,'h001'))?.company_id).toBe('beacon-co');
 expect((await readCompanyPreference(db,user,'h002'))?.company_id).toBe('northstar-co');
 expect(await readWorkspacePreference(db,other)).toBeNull();
 expect(await readCompanyPreference(db,other,'h001')).toBeNull();
 await rememberPlatformRole(db,user,'platform-role');await rememberWorkspace(db,user,'/platform/home');
 expect(await readWorkspacePreference(db,user)).toMatchObject({workspace_path:'/platform/home',platform_assignment_id:'platform-role'});
 expect((await readCompanyPreference(db,user,'h001'))?.company_id).toBe('beacon-co');
});
