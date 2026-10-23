import {beforeEach,expect,it,vi} from 'vitest';
import type {AccountAccessContext} from '@erpx/server-identity_access/public/authorization';
vi.mock('server-only',()=>({}));
const mocks=vi.hoisted(()=>({access:vi.fn(),modules:vi.fn(),iam:vi.fn(),admin:vi.fn(),selection:vi.fn(),select:vi.fn(),platform:vi.fn(),ready:vi.fn(),current:vi.fn(),beta:vi.fn()}));
vi.mock("@erpx/server-identity_access/adapters/authorization",()=>({requireAccountAccess:mocks.access,requirePlatformContext:mocks.platform}));
vi.mock("@erpx/server-identity_access/adapters/work-module-access",()=>({accountWorkModuleAccess:mocks.modules}));
vi.mock("@erpx/server-identity_access/adapters/beta-access",()=>({betaAccess:mocks.beta}));
vi.mock("@erpx/server-identity_access/adapters/iam-store",()=>({loadIamAuthority:mocks.iam}));
vi.mock("@erpx/server-runtime/public/administration",()=>({administrationAccess:mocks.admin}));
vi.mock("@erpx/server-pms/public/feature-availability",()=>({assertProductionModuleEnabled:mocks.ready}));
vi.mock("@erpx/server-runtime/public/runtime",()=>({holdingResources:async()=>({write:{prepare:()=>({bind:()=>({first:mocks.current})})}})}));
vi.mock("@erpx/server-work_projection/public/work-role-store",()=>({initializeWorkSelection:mocks.selection,selectWorkRole:mocks.select}));
vi.mock("@erpx/server-organization/public/product-company",()=>({productCompany:async()=>({id:'company',name:'Current Company'})}));
vi.mock("@erpx/feature-runtime/lib/product-hosts",()=>({productHostForHost:(host:string)=>host==='admin-dev-erpx.jesadakorn.com'?'admin':null,configuredProductOrigin:()=> 'https://admin-dev-erpx.jesadakorn.com'}));
import {adminWorkspaceTarget,adminTargetAllowed} from '@erpx/server-identity_access/domain/admin-workspace';
import {adminWorkspaceRequest,adminManagementAvailable,adminWorkRoleChoices,adminWorkspaceAvailabilityReader,adminWorkspaceDestinations} from '@erpx/server-identity_access/adapters/admin-workspace';
import {ApiError} from '@erpx/server-runtime/public/http';
import {builtInPermissions} from '@erpx/feature-identity_access/lib/permissions';
const access=():AccountAccessContext=>({scope:{kind:'holding',holdingSlug:'acme',holdingId:'h001',holdingName:'Acme',companyId:'company'},sessionId:'session',expiresAt:Date.now()+60000,userId:'user',name:'User',email:'fixture@example.test',requestHeaders:new Headers(),assignments:[{id:'admin',role:'holding_owner',companyId:'company',branchId:null,modules:['identity_access','brand'],scopeKind:'holding',scopeNodeId:null,version:2},{id:'finance',role:'finance_preparer',companyId:'company',branchId:null,modules:['finance'],scopeKind:'holding',scopeNodeId:null,version:1}]});
const selected=(id='admin')=>({session_id:'session',user_id:'user',company_id:'company',assignment_id:id,assignment_version:id==='admin'?2:1,revision:4,expires_at:Date.now()+60000});
const target='/holdings/acme/administration/technology/settings/roles?q=own#draft';
const get=(next=target,host='admin-dev-erpx.jesadakorn.com')=>new Request(`https://${host}/admin/launch?`+new URLSearchParams({companyId:'company',next}),{headers:{host}});
const post=(patch:Record<string,string>={},origin='https://admin-dev-erpx.jesadakorn.com')=>new Request(origin+'/admin/launch',{method:'POST',headers:{host:'admin-dev-erpx.jesadakorn.com',origin,'content-type':'application/x-www-form-urlencoded'},body:new URLSearchParams({companyId:'company',next:target,assignmentId:'admin',assignmentVersion:'2',revision:'4',...patch})});
beforeEach(()=>{for(const mock of Object.values(mocks))mock.mockReset();mocks.beta.mockResolvedValue(false);mocks.access.mockResolvedValue(access());mocks.modules.mockResolvedValue({modulesByAssignment:new Map([['admin',['identity_access','brand']],['finance',['finance']]]),presentationModulesByAssignment:new Map(),permissionsByAssignment:new Map(),beta:false});mocks.iam.mockImplementation(async(_db,_company,_user,role)=>({permissions:builtInPermissions(role)}));mocks.admin.mockResolvedValue({sections:['brand','organization','approvals','usage','ai','feedback'],managePeople:true,manageOrganization:true,owner:true,manageAiProviders:true});mocks.selection.mockResolvedValue(selected());mocks.current.mockResolvedValue({one:1});mocks.platform.mockResolvedValue({userId:'user'});});
it('rejects external, wrong surface, wrong Holding management and malformed Company targets',()=>{for(const value of ['https://evil.test/holdings/acme/administration','/holdings/acme/finance/documents','/platform/users','/holdings/acme/erpx-management','/holdings/erpx/branches/a/erpx-management','/holdings/acme/administration%2ftechnology'])expect(adminWorkspaceTarget('company',value)).toBeNull();expect(adminWorkspaceTarget(null,target)).toBeNull();expect(adminWorkspaceTarget('company',target)?.target).toBe(target);});
it('enters current Company Admin without a product purchase and preserves query, fragment and host-only cookie',async()=>{const response=await adminWorkspaceRequest(get());expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://admin-dev-erpx.jesadakorn.com'+target);expect(mocks.access).toHaveBeenCalledWith({kind:'holding',holdingSlug:'acme'},{companyId:'company'});expect(response.headers.get('set-cookie')).toContain('erpx_company_h001=company');expect(response.headers.get('set-cookie')).not.toMatch(/domain=/i);expect(response.headers.get('cache-control')).toBe('private, no-store');expect(mocks.select).not.toHaveBeenCalled();});
it('keeps a different live role unchanged until an explicit same-origin versioned choice',async()=>{mocks.selection.mockResolvedValue(selected('finance'));const response=await adminWorkspaceRequest(get());expect(new URL(response.headers.get('location')!).pathname).toBe('/admin/choose-work-role');expect(mocks.select).not.toHaveBeenCalled();const chosen=await adminWorkspaceRequest(post());expect(new URL(chosen.headers.get('location')!).pathname).toBe('/admin/launch');expect(mocks.select).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({assignmentId:'admin',assignmentVersion:2,revision:4,companyId:'company'}));});
it('denies CSRF, duplicate fields, wrong host, stale role and stale context without selecting',async()=>{expect((await adminWorkspaceRequest(post({},'https://evil.test'))).status).toBe(403);expect((await adminWorkspaceRequest(get(target,'finance-dev-erpx.jesadakorn.com'))).status).toBe(400);const patches:Record<string,string>[]=[{assignmentVersion:'1'},{revision:'3'},{assignmentId:'foreign'}];for(const patch of patches){const response=await adminWorkspaceRequest(post(patch));expect(response.status).toBe(303);expect(new URL(response.headers.get('location')!).searchParams.get('notice')).toBe('access-changed');}const duplicate=post();expect((await adminWorkspaceRequest(new Request(duplicate,{body:await duplicate.text()+'&revision=4'}))).status).toBe(422);expect(mocks.select).not.toHaveBeenCalled();});
it('rechecks original release, current assignment and exact Company and exposes only normal displayed role choices',async()=>{const data=await adminWorkRoleChoices(adminWorkspaceTarget('company',target)!);expect(data.options).toEqual([{assignmentId:'admin',assignmentVersion:2,label:'holding owner'}]);expect(JSON.stringify(data)).not.toMatch(/permissions|modules|session|roleRevision|assignments/);mocks.ready.mockRejectedValue(new ApiError(403,'FEATURE_COMING_SOON','Unreleased'));expect((await adminWorkspaceRequest(get())).status).toBe(403);expect(mocks.select).not.toHaveBeenCalled();mocks.ready.mockResolvedValue(undefined);mocks.current.mockResolvedValue(null);expect((await adminWorkspaceRequest(get())).status).toBe(403);mocks.current.mockResolvedValue({one:1});mocks.access.mockResolvedValue({...access(),scope:{...access().scope,companyId:'other'}});expect((await adminWorkspaceRequest(get())).status).toBe(403);});
it('requires actual matching Platform plus ERPX Holding permission for management without selecting a role',async()=>{const account=access(),assignment=account.assignments[0];expect(await adminManagementAvailable(account,assignment)).toBe(false);const erpx={...account,scope:{...account.scope,holdingSlug:'erpx',holdingId:'h003' as const}};expect(await adminManagementAvailable(erpx,assignment)).toBe(true);mocks.platform.mockResolvedValue({userId:'other'});expect(await adminManagementAvailable(erpx,assignment)).toBe(false);mocks.platform.mockResolvedValue({userId:'user'});expect(await adminManagementAvailable(erpx,erpx.assignments[1])).toBe(false);expect(mocks.select).not.toHaveBeenCalled();});

it('guards the native Admin role form with the shared dirty check before context switching',async()=>{
 const {submitAdminRoleChoice}=await import('@erpx/feature-identity_access/components/admin-role-choice-form'),{setContextSwitching,contextIsSwitching}=await import('@erpx/feature-work_projection/lib/context-switch'),confirm=vi.fn(()=>false),send=vi.fn(),assign=vi.fn();
 vi.stubGlobal('document',{querySelector:()=>({dirty:true}),documentElement:{dataset:{}}});
 vi.stubGlobal('window',{confirm,dispatchEvent:vi.fn(),location:{origin:'https://admin-dev-erpx.jesadakorn.com',assign}});
 vi.stubGlobal('FormData',class{*[Symbol.iterator](){yield ['assignmentId','existing-owner'];yield ['revision','4'];}});
 vi.stubGlobal('fetch',send);
 try{
  expect(await submitAdminRoleChoice({} as HTMLFormElement)).toBe(false);expect(contextIsSwitching()).toBe(false);expect(send).not.toHaveBeenCalled();
  confirm.mockReturnValue(true);send.mockResolvedValue({ok:true,redirected:true,url:'https://admin-dev-erpx.jesadakorn.com/holdings/simulation/administration'});
  expect(await submitAdminRoleChoice({} as HTMLFormElement)).toBe(true);expect(contextIsSwitching()).toBe(true);
  expect(send).toHaveBeenCalledWith('/admin/launch',expect.objectContaining({method:'POST',mode:'cors',credentials:'same-origin',referrerPolicy:'no-referrer'}));
  expect(send.mock.calls[0][1].body.toString()).toBe('assignmentId=existing-owner&revision=4');
  expect(assign).toHaveBeenCalledWith('https://admin-dev-erpx.jesadakorn.com/holdings/simulation/administration');
  expect(await submitAdminRoleChoice({} as HTMLFormElement)).toBe(false);expect(send).toHaveBeenCalledTimes(1);
  for(const response of [{ok:false},{ok:true,redirected:false,url:'https://admin-dev-erpx.jesadakorn.com/'},{ok:true,redirected:true,url:'https://foreign.invalid/'}]){
   setContextSwitching(false);send.mockResolvedValue(response);await expect(submitAdminRoleChoice({} as HTMLFormElement)).rejects.toThrow();expect(contextIsSwitching()).toBe(false);
  }
  expect(assign).toHaveBeenCalledTimes(1);
 }finally{setContextSwitching(false);vi.unstubAllGlobals();}
});

it('reads only the prospective role for discovery while normal role choices retain their fresh full module scans',async()=>{
 const account=access(),input=adminWorkspaceTarget('company',target)!;mocks.access.mockResolvedValue(account);
 expect(await adminWorkspaceAvailabilityReader()(input)).toBe(true);
 expect(mocks.modules).not.toHaveBeenCalled();expect(mocks.beta).toHaveBeenCalledTimes(1);
 expect(mocks.iam).toHaveBeenCalledTimes(1);expect(mocks.current).toHaveBeenCalledTimes(1);expect(mocks.selection).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();
 const normal=await adminWorkRoleChoices(input);expect(normal.options).toEqual([{assignmentId:'admin',assignmentVersion:2,label:'holding owner'}]);
 expect(mocks.modules).toHaveBeenCalledTimes(2);
 expect(mocks.iam).toHaveBeenCalledTimes(3);expect(mocks.current).toHaveBeenCalledTimes(3);
 expect(await adminWorkspaceAvailabilityReader()(input)).toBe(true);expect(mocks.modules).toHaveBeenCalledTimes(2);expect(mocks.beta).toHaveBeenCalledTimes(2);
});

it('stops boolean discovery after one fully allowed current role without reading an unrelated later role',async()=>{
 const input=adminWorkspaceTarget('company',target)!;mocks.iam.mockImplementation(async(_db,_company,_user,role)=>{if(role==='finance_preparer')throw Error('Later role source must stay unread');return {permissions:builtInPermissions(role)};});
 expect(await adminWorkspaceAvailabilityReader()(input)).toBe(true);expect(mocks.iam).toHaveBeenCalledTimes(1);expect(mocks.current).toHaveBeenCalledTimes(1);expect(mocks.admin).toHaveBeenCalledTimes(1);expect(mocks.ready).toHaveBeenCalled();expect(mocks.selection).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();
 await expect(adminWorkRoleChoices(input)).rejects.toThrow('Later role source must stay unread');
});

it('checks every candidate role when none is current and preserves the original boolean discovery denial',async()=>{
 mocks.current.mockResolvedValue(null);await expect(adminWorkspaceAvailabilityReader()(adminWorkspaceTarget('company',target)!)).rejects.toMatchObject({status:403,code:'ADMIN_ROLE_REQUIRED'});
 expect(mocks.modules).not.toHaveBeenCalled();expect(mocks.beta).toHaveBeenCalledTimes(1);expect(mocks.iam).toHaveBeenCalledTimes(2);expect(mocks.current).toHaveBeenCalledTimes(2);expect(mocks.admin).not.toHaveBeenCalled();expect(mocks.selection).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();
});

it('does not reuse discovery Beta facts for a different actual context even at the same Company',async()=>{
 const available=adminWorkspaceAvailabilityReader(),first=access(),second={...access(),userId:'other-user',sessionId:'other-session',assignments:access().assignments.map(row=>({...row,version:row.version!+1}))},input=adminWorkspaceTarget('company',target)!;
 mocks.access.mockResolvedValueOnce(first).mockResolvedValueOnce(second);mocks.beta.mockResolvedValueOnce(false).mockResolvedValueOnce(true);mocks.iam.mockImplementation(async(_db,_company,user,role)=>{if(user==='other-user')throw new ApiError(403,'ROLE_NOT_ACTIVE','Original current role unavailable');return {permissions:builtInPermissions(role)};});
 expect(await available(input)).toBe(true);await expect(available(input)).rejects.toMatchObject({code:'ADMIN_ROLE_REQUIRED'});
 expect(mocks.beta).toHaveBeenCalledTimes(2);expect(mocks.modules).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();
});

it('excludes a denied prospective IAM role and continues to a current fallback while unexpected source errors propagate',async()=>{
 const account=access();account.assignments.reverse();mocks.access.mockResolvedValue(account);mocks.iam.mockImplementation(async(_db,_company,_user,role)=>{if(role==='finance_preparer')throw new ApiError(403,'ROLE_NOT_ACTIVE','Original current role unavailable');return {permissions:builtInPermissions(role)};});
 expect(await adminWorkspaceAvailabilityReader()(adminWorkspaceTarget('company',target)!)).toBe(true);expect(mocks.beta).toHaveBeenCalledTimes(1);expect(mocks.modules).not.toHaveBeenCalled();expect(mocks.iam).toHaveBeenCalledTimes(2);expect(mocks.current).toHaveBeenCalledTimes(1);expect(mocks.admin).toHaveBeenCalledTimes(1);expect(mocks.select).not.toHaveBeenCalled();
 mocks.iam.mockRejectedValue(Error('Original IAM source failure'));await expect(adminWorkspaceAvailabilityReader()(adminWorkspaceTarget('company',target)!)).rejects.toThrow('Original IAM source failure');
});

it('reconstructs the same owner actor and destinations as the fresh normal reader under both original Company Beta states',async()=>{
 const account=access(),assignment=account.assignments[0],input=adminWorkspaceTarget('company',target)!;
 for(const beta of [false,true]){
  mocks.access.mockResolvedValue(account);mocks.beta.mockResolvedValue(beta);mocks.modules.mockResolvedValue({modulesByAssignment:new Map([['admin',['identity_access','brand']],['finance',['finance']]]),presentationModulesByAssignment:new Map(),permissionsByAssignment:new Map(),beta});mocks.admin.mockClear();
  const normal=await adminWorkspaceDestinations(account,assignment),actor=mocks.admin.mock.calls[0][0];mocks.admin.mockClear();
  const reading=adminWorkspaceAvailabilityReader()(input);if(adminTargetAllowed(input,normal.map(item=>item.path)))expect(await reading).toBe(true);else await expect(reading).rejects.toMatchObject({code:'ADMIN_ROLE_REQUIRED'});
  expect(mocks.admin.mock.calls[0][0]).toEqual(actor);expect(actor.beta).toBe(beta);expect(mocks.select).not.toHaveBeenCalled();
 }
});
