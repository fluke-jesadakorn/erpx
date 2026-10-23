import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {env,applyMigrations} from "../../support/runtime-env";
import {iamDb} from "@erpx/server-identity_access/public/iam-db";
import {loadIamAuthority} from "@erpx/server-identity_access/public/iam-store";
import {sealSecret} from "@erpx/server-identity_access/public/iam-mfa";
import type {IamActor} from "@erpx/server-identity_access/public/iam-context";
import {connectionInput,saveConnection,testConnection,proposeConnectionActivation,disableConnection,applyDirectoryReview} from "@erpx/server-identity_access/public/iam-directories";
import {connectionById} from '@erpx/server-identity_access/public/directory-lookup';
import {decideIamReview,grantIamAccess,grantInput,revokeIamAssignment,iamAuditHistory} from "@erpx/server-identity_access/public/iam-service";
import {iamCommand} from "@erpx/server-identity_access/public/iam-transactions";
import {prepareLocalBrowserRealm} from "../../../fixtures/shared/builders/local-browser-erpx-realm";
import {initializeWorkSelection} from "@erpx/server-work_projection/public/work-role-store";
const adapter=vi.hoisted(()=>({test:vi.fn()}));
vi.mock("@erpx/server-identity_access/public/iam-providers",async()=>({...await vi.importActual<typeof import("@erpx/server-identity_access/public/iam-providers")>("@erpx/server-identity_access/public/iam-providers"),testDirectory:adapter.test}));
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({runtimeEnv:async()=>env}));
const db=iamDb(env.CONTROL_DB),key=()=>crypto.randomUUID();
beforeAll(async()=>{await applyMigrations(db,env.CONTROL_TEST_MIGRATIONS);});
beforeEach(()=>{adapter.test.mockReset();adapter.test.mockResolvedValue({userCount:1,groupCount:0,snapshot:{complete:true,users:[{externalId:'fixture-user',username:'fixture',name:'Declared directory fixture',enabled:true,groupIds:[]}],groups:[]}});});
async function account(role='platform_admin',withAssignment=true):Promise<IamActor>{
 const userId=key(),assignmentId=key(),sessionId=key(),now=Date.now();
 await db.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind(userId,'Platform '+role,userId+'@example.test',now,now).run();
 await db.prepare('INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES(?,?,?,?,?,?)').bind(sessionId,new Date(now+86400000).toISOString(),key(),new Date(now).toISOString(),new Date(now).toISOString(),userId).run();
 await db.prepare('INSERT INTO iam_mfa(user_id,secret,enabled) VALUES(?,?,true)').bind(userId,await sealSecret('JBSWY3DPEHPK3PXP')).run();await db.prepare('INSERT INTO iam_mfa_sessions VALUES(?,?,?)').bind(sessionId,userId,now).run();
 if(withAssignment)await db.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until) VALUES(?,?,?,'[]',0,?)").bind(assignmentId,userId,role,now+30*86400000).run();
 return {userId,assignmentId,sessionId,companyId:'platform',role,holdingId:null,branchId:null,departmentId:null,scopeKind:'platform',scopeNodeId:null,platform:true,iam:await loadIamAuthority(db,'platform',userId,role,sessionId)};
}
const configuration=(extra:Record<string,unknown>={})=>connectionInput.parse({expectedVersion:0,name:'Declared fixture connector',provider:'univention',config:{mode:'ldaps',secretProfile:'declared-fixture',authority:'corp.test',ldapUrl:'ldaps://ucs.corp.test',baseDn:'dc=corp,dc=test'},...extra});
it('registers, tests, independently activates and disables a Platform connector while preserving current configuration and fixture evidence',async()=>{
 const owner=await account(),reviewer=await account(),auditor=await account('platform_auditor'),saveKey=key(),input=configuration();
 const saved=await saveConnection(db,owner,input,saveKey);expect(await saveConnection(db,owner,input,saveKey)).toEqual(saved);expect((await connectionById(db,'platform',saved.id)).status).toBe('draft');expect(await db.prepare('SELECT count(*) n FROM iam_identities WHERE connection_id=?').bind(saved.id).first('n')).toBe(0);
 await expect(proposeConnectionActivation(db,owner,saved.id,1,key())).rejects.toMatchObject({code:'CONNECTION_TEST_REQUIRED'});await expect(testConnection(db,auditor,saved.id,1,key())).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});expect(adapter.test).not.toHaveBeenCalled();
 expect(await testConnection(db,owner,saved.id,1,key())).toMatchObject({id:saved.id,status:'tested',userCount:1});const proposed=await proposeConnectionActivation(db,owner,saved.id,1,key());
 await expect(decideIamReview(db,owner,proposed.id,{expectedVersion:1,decision:'approve',reason:'Forbidden self connector activation'},key(),(client,kind,payload)=>applyDirectoryReview(client,owner,kind,payload))).rejects.toMatchObject({code:'INDEPENDENT_REVIEW_REQUIRED'});
 const reviewKey=key();await decideIamReview(db,reviewer,proposed.id,{expectedVersion:1,decision:'approve',reason:'Independent declared fixture trust review'},reviewKey,(client,kind,payload)=>applyDirectoryReview(client,reviewer,kind,payload));expect((await connectionById(db,'platform',saved.id)).status).toBe('active');
 await expect(saveConnection(db,owner,configuration({id:saved.id,expectedVersion:1}),key())).rejects.toMatchObject({code:'CONNECTION_ACTIVE'});
 const disableKey=key();expect(await disableConnection(db,owner,saved.id,1,disableKey)).toMatchObject({id:saved.id,status:'disabled'});expect(await disableConnection(db,owner,saved.id,1,disableKey)).toMatchObject({id:saved.id,status:'disabled'});expect((await connectionById(db,'platform',saved.id)).version).toBe(2);
 expect(await db.prepare("SELECT count(*) n FROM iam_audit WHERE company_id='platform' AND action='connection.disable' AND target_id=?").bind(disableKey).first('n')).toBe(1);
 expect((await iamAuditHistory(db,reviewer,'connection.activate.propose')).items).toContainEqual(expect.objectContaining({action:'connection.activate.propose',evidence:expect.objectContaining({input:expect.objectContaining({id:saved.id})})}));
});
it('refuses a connector result returned for stale configuration and records provider failure without activating it',async()=>{
 const owner=await account(),saved=await saveConnection(db,owner,configuration(),key());adapter.test.mockImplementationOnce(async()=>{await saveConnection(db,owner,configuration({id:saved.id,expectedVersion:1,name:'Edited while fixture provider pending'}),key());return {userCount:0,groupCount:0,snapshot:{complete:true,users:[],groups:[]}};});
 await expect(testConnection(db,owner,saved.id,1,key())).rejects.toMatchObject({code:'CONNECTION_CHANGED'});expect(await connectionById(db,'platform',saved.id)).toMatchObject({status:'draft',version:2,tested_version:null,health:'untested'});
 adapter.test.mockRejectedValueOnce(new Error('Declared fixture certificate failure'));await expect(testConnection(db,owner,saved.id,2,key())).rejects.toMatchObject({code:'DIRECTORY_TEST_FAILED'});expect(await connectionById(db,'platform',saved.id)).toMatchObject({status:'draft',health:'error',last_error:'DIRECTORY_TEST_FAILED'});
 await expect(proposeConnectionActivation(db,owner,saved.id,2,key())).rejects.toMatchObject({code:'CONNECTION_TEST_REQUIRED'});
});
it('reviews temporary Platform grants independently and refuses current reads, mutations and replay immediately after expiry or revocation',async()=>{
 const owner=await account(),reviewer=await account(),target=await account('platform_auditor',false),until=Date.now()+60000,input=grantInput.parse({userId:target.userId,roleId:'platform_auditor',scopeKind:'platform',validUntil:until,reason:'Time limited engineering inspection'}),grantKey=key();
 const proposed=await grantIamAccess(db,owner,input,grantKey);expect(await grantIamAccess(db,owner,input,grantKey)).toEqual(proposed);
 await expect(decideIamReview(db,owner,proposed.id,{expectedVersion:1,decision:'approve',reason:'Forbidden self approval'},key())).rejects.toMatchObject({code:'INDEPENDENT_REVIEW_REQUIRED'});await decideIamReview(db,reviewer,proposed.id,{expectedVersion:1,decision:'approve',reason:'Independent Platform read-only expiry review'},key());
 const grant=await db.prepare('SELECT id,version,parent_id,valid_until FROM platform_assignments WHERE user_id=?').bind(target.userId).first<{id:string;version:number;parent_id:string;valid_until:number}>();expect(grant).toMatchObject({parent_id:owner.assignmentId,valid_until:until});target.assignmentId=grant!.id;target.iam=await loadIamAuthority(db,'platform',target.userId,target.role,target.sessionId);
 const readKey=key(),read=()=>iamCommand(db,target,readKey,'engineering.temporary.read',{},'iam.read',async()=>({records:'approved read-only inspection'}));expect(await read()).toEqual({records:'approved read-only inspection'});await expect(iamCommand(db,target,key(),'engineering.temporary.write',{},'iam.manage',async()=>({forbidden:true}))).rejects.toMatchObject({code:'PERMISSION_REQUIRED'});
 const realNow=Date.now;try{Date.now=()=>until+1;await expect(read()).rejects.toMatchObject({code:'ASSIGNMENT_UNAVAILABLE'});}finally{Date.now=realNow;}
 const revokeKey=key();await revokeIamAssignment(db,reviewer,grant!.id,1,'Inspection complete; withdraw this exact temporary grant',revokeKey);await revokeIamAssignment(db,reviewer,grant!.id,1,'Inspection complete; withdraw this exact temporary grant',revokeKey);await expect(read()).rejects.toMatchObject({code:'ASSIGNMENT_UNAVAILABLE'});
 expect(await db.prepare("SELECT iam_assignment_active(?,?) active").bind(grant!.id,Date.now()).first('active')).toBe(false);expect(await db.prepare("SELECT count(*) n FROM iam_audit WHERE action='assignment.revoke' AND target_id=?").bind(revokeKey).first('n')).toBe(1);
 expect(await db.prepare('SELECT count(*) n FROM iam_receipts WHERE id=?').bind(readKey).first('n')).toBe(1);
});

it('prepares only the disposable ERPX realm for the existing Platform identity and retains normal owner authority',async()=>{
 const realm=env.H003_DB;await applyMigrations(realm,env.STAFF_TEST_MIGRATIONS);
 const before=await realm.prepare('SELECT count(*)::int n FROM assignments').first<{n:number}>();
 await expect(prepareLocalBrowserRealm(env.CONTROL_DB,realm)).rejects.toThrow('EXISTING_LOCAL_BROWSER_PLATFORM_IDENTITY_REQUIRED');
 expect(await realm.prepare('SELECT count(*)::int n FROM assignments').first()).toEqual(before);
 const now=Date.now();
 await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)').bind('usr_jesadakorn','Existing fixture owner','existing-realm-owner@example.test',now,now).run();
 await env.CONTROL_DB.prepare("INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until) VALUES('platform_usr_jesadakorn','usr_jesadakorn','platform_admin','[]',?,?)").bind(now-1000,now+86400000).run();
 const platformBefore=(await env.CONTROL_DB.prepare("SELECT * FROM platform_assignments WHERE user_id='usr_jesadakorn'").all()).results;
 const prepared=await prepareLocalBrowserRealm(env.CONTROL_DB,realm);
 expect(await prepareLocalBrowserRealm(env.CONTROL_DB,realm)).toEqual(prepared);
 expect((await env.CONTROL_DB.prepare("SELECT * FROM platform_assignments WHERE user_id='usr_jesadakorn'").all()).results).toEqual(platformBefore);
 expect((await loadIamAuthority(realm,prepared.companyId,prepared.userId,'holding_owner')).permissions).toContain('manage_erpx.access');
 const eligible=(await realm.prepare("SELECT a.id,a.version FROM assignments a JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active' WHERE a.user_id=? AND a.company_id=? AND a.scope_kind='holding' AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)").bind(prepared.userId,prepared.companyId,now,now).all<{id:string;version:number}>()).results;
 expect(eligible.map(a=>a.id)).toEqual([prepared.assignmentId]);
 const selection=await initializeWorkSelection(realm,{sessionId:key(),userId:prepared.userId,companyId:prepared.companyId,expiresAt:now+86400000},eligible,eligible);
 expect(selection).toMatchObject({assignment_id:prepared.assignmentId});
 expect(await env.CONTROL_DB.prepare("SELECT slug,write_binding,read_binding,files_binding FROM holding_bindings WHERE holding_id='h003'").first()).toEqual({slug:'erpx',write_binding:'H003_DB',read_binding:'H003_READ_DB',files_binding:'H003_FILES'});
 expect(await realm.prepare('SELECT count(*)::int n FROM assignments').first()).toEqual({n:before!.n+1});
});
