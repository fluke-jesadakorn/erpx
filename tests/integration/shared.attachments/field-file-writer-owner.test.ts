import {beforeAll,describe,expect,it,vi} from 'vitest';
import {randomUUID} from 'node:crypto';
import {applyMigrations,env} from '../../support/runtime-env';
import {fieldFileWriterOwnerInDatabase} from '@erpx/server-files/public/field-file-writer-owner';

const routeState=vi.hoisted(()=>({portalReader:vi.fn(),legacy:vi.fn(async(_db:unknown,_actor:unknown,id:string)=>({linkId:id,owner:'other'}))}));
vi.mock('@erpx/server-customer_authority/public/attachment-read',()=>({withPortalAttachmentContentRead:(...args:unknown[])=>routeState.portalReader(...args)}));
vi.mock('@erpx/server-runtime/public/runtime',()=>({holdingResources:async()=>({write:env.H001_DB})}));
vi.mock('@erpx/server-identity_access/public/authorization',()=>({requireWorkContext:async()=>({holdingId:'h001',holdingSlug:'acme',companyId:'owner-company'})}));
vi.mock('@erpx/server-files/public/attachments/service',()=>({attachmentLinkStatus:routeState.legacy}));
vi.mock('../../../packages/workplace/src/lib/native-field-family',()=>({nativeFieldFamily:async(_request:Request,_actor:unknown,input:{route:{kind:'status';linkId:string}})=>{
 const {fieldFilesRoute}=await import('../../../packages/native-sdk/src/field-service-files-routes');
 return Response.json({nativePath:fieldFilesRoute('acme',input.route).path});
}}));
import {GET as statusRoute} from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/status/route';

const db=env.H001_DB, prefix=`field-owner-${randomUUID()}`, actor={companyId:'owner-company'};
const field=`${prefix}-field`,other=`${prefix}-other`,foreign=`${prefix}-foreign`,orphan=`${prefix}-orphan`,linked=`${prefix}-linked`,corrupt=`${prefix}-corrupt`,foreignUpload=`${prefix}-foreign-upload`;
const mixedOther=`${prefix}-mixed-other`;
const longFieldLink=`${prefix}-${'f'.repeat(215)}`,longOtherLink=`${prefix}-${'o'.repeat(215)}`;
beforeAll(async()=>{
 await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);
 for(const [id,company,type] of [[field,actor.companyId,'field_service_visit'],[other,actor.companyId,'asset'],[foreign,'foreign-company','field_service_visit'],[mixedOther,actor.companyId,'asset']]){
  await db.prepare("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES(?,?,?,?,'record','site_photo',1,1,'internal','{}','tester',1,1)").bind(id,company,type,`${id}-target`).run();
 }
 await db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES(?,'field_service_visit','missing','site_photo','a.png','image/png',1,?,'pending','tester',100,1,1,?)").bind(orphan,`${orphan}-key`,actor.companyId).run();
 await db.prepare("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES(?,?,'draft','tester',1,100)").bind(linked,field).run();
 await db.prepare("INSERT INTO attachment_links(id,set_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft','tester',1,100)").bind(corrupt,mixedOther,orphan).run();
 await db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES(?,'field_service_visit','missing','site_photo','a.png','image/png',1,?,'pending','tester',100,1,1,'foreign-company')").bind(foreignUpload,`${foreignUpload}-key`).run();
 for(const [id,set] of [[longFieldLink,field],[longOtherLink,other]])await db.prepare("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES(?,?,'draft','tester',1,100)").bind(id,set).run();
});
describe('persisted Field file writer selection',()=>{
 it('selects Field sets, links and orphaned Field upload markers without granting access',async()=>{
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'set',id:field})).resolves.toBe('field');
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'link',id:linked})).resolves.toBe('field');
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'upload',id:orphan})).resolves.toBe('field');
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'link',id:corrupt})).resolves.toBe('field');
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'set',id:mixedOther})).resolves.toBe('field');
 });
 it('preserves non-Field and absent records, and denies foreign Field markers opaquely',async()=>{
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'set',id:other})).resolves.toBe('other');
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'link',id:`${prefix}-missing`})).resolves.toBe('other');
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'upload',id:foreignUpload})).rejects.toMatchObject({status:404,code:'ATTACHMENT_NOT_FOUND'});
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'set',id:foreign})).rejects.toMatchObject({status:404,code:'ATTACHMENT_NOT_FOUND'});
  await expect(fieldFileWriterOwnerInDatabase(db,actor,{kind:'set',id:''})).rejects.toMatchObject({status:404});
 });
});

it('routes long persisted physical IDs through actual Company ownership and SDK paths',async()=>{
 routeState.legacy.mockClear();
 const read=(id:string)=>statusRoute(new Request(`https://acme.example.test/api/v1/holdings/acme/attachment-links/${encodeURIComponent(id)}/status`),{params:Promise.resolve({holdingSlug:'acme',linkId:id})});
 const fieldResponse=await read(longFieldLink);expect(fieldResponse.status).toBe(200);
 expect(await fieldResponse.json()).toEqual({nativePath:`/api/v1/holdings/acme/field-service/attachment-links/${encodeURIComponent(longFieldLink)}/status`});expect(routeState.legacy).not.toHaveBeenCalled();
 const otherResponse=await read(longOtherLink);expect(otherResponse.status).toBe(200);expect((await otherResponse.json()).data).toEqual({linkId:longOtherLink,owner:'other'});expect(routeState.legacy).toHaveBeenCalledOnce();
});

it('rejects retired direct Field content calls without opening bytes or recording allowed access',async()=>{
 const {seedAttachmentSet}=await import('../../support/helpers/attachment-v2');
 const current={companyId:actor.companyId,userId:'tester',holdingId:'h001',holdingSlug:'acme'} as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const source=await seedAttachmentSet(db,current,{targetType:'field_service_visit',targetId:`${prefix}-target`,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1});
 const {attachmentContent}=await vi.importActual<typeof import('@erpx/server-files/public/attachments/service')>('@erpx/server-files/public/attachments/service');
 const get=vi.fn(()=>{throw Error('Retired Field implementation opened private bytes');});
 const store={get} as unknown as import('@erpx/server-runtime/public/runtime/types').FileStore;
 const before=await db.prepare('SELECT count(*)::int count FROM attachment_access_events WHERE link_id=?').bind(source.linkId).first('count');
 await expect(attachmentContent(db,store,current,source.linkId,'original')).rejects.toMatchObject({status:503,code:'FIELD_FILES_NATIVE_REQUIRED'});
 expect(get).not.toHaveBeenCalled();expect(await db.prepare('SELECT count(*)::int count FROM attachment_access_events WHERE link_id=?').bind(source.linkId).first('count')).toBe(before);
});

it('keeps mixed persisted Field markers out of every shared legacy read entry',async()=>{
 const {seedAttachmentSet}=await import('../../support/helpers/attachment-v2');
 const current={companyId:actor.companyId,userId:'tester',holdingId:'h001',holdingSlug:'acme'} as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const source=await seedAttachmentSet(db,current,{targetType:'field_service_visit',targetId:`${prefix}-mixed-source`,componentKind:'record',purposeKey:'technician_report',expectedTargetVersion:1});
 const mixedSet=`${prefix}-mixed-set`,mixedLink=`${prefix}-mixed-link`,target=`${prefix}-mixed-target`;
 await db.prepare("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES(?,?,'asset',?,'record','site_photo',1,1,'internal','{}','tester',1,1)").bind(mixedSet,actor.companyId,target).run();
 await db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft','tester',1,100)").bind(mixedLink,mixedSet,source.sourceId).run();
 expect(await fieldFileWriterOwnerInDatabase(db,actor,{kind:'link',id:mixedLink})).toBe('field');expect(await fieldFileWriterOwnerInDatabase(db,actor,{kind:'set',id:mixedSet})).toBe('field');
 const actual=await vi.importActual<typeof import('@erpx/server-files/public/attachments/service')>('@erpx/server-files/public/attachments/service');
 const get=vi.fn(()=>{throw Error('Mixed Field lineage opened legacy bytes');}),store={get} as unknown as import('@erpx/server-runtime/public/runtime/types').FileStore;
 const before=await db.prepare('SELECT count(*)::int count FROM attachment_access_events WHERE link_id=?').bind(mixedLink).first('count');
 for(const operation of [()=>actual.attachmentContent(db,store,current,mixedLink,'original'),()=>actual.attachmentLinkStatus(db,current,mixedLink),()=>actual.listAttachmentSets(db,current,{targetType:'asset',targetId:target})])await expect(operation()).rejects.toMatchObject({status:503,code:'FIELD_FILES_NATIVE_REQUIRED'});
 expect(get).not.toHaveBeenCalled();expect(await db.prepare('SELECT count(*)::int count FROM attachment_access_events WHERE link_id=?').bind(mixedLink).first('count')).toBe(before);
});

it('rechecks ownership after locked reader handoff and inside alternate Portal metadata units',async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2');enableAttachmentV2ForTest();
 const current={companyId:actor.companyId,userId:'tester',holdingId:'h001',holdingSlug:'acme'} as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const portal=await seedAttachmentSet(db,current,{targetType:'customer_access_binding',targetId:`${prefix}-portal`,componentKind:'action',purposeKey:'customer_support_evidence',expectedTargetVersion:1});
 const actual=await vi.importActual<typeof import('@erpx/server-files/public/attachments/service')>('@erpx/server-files/public/attachments/service');
 const get=vi.fn(()=>{throw Error('Ownership handoff reached physical bytes');}),store={get} as unknown as import('@erpx/server-runtime/public/runtime/types').FileStore;
 const before=await db.prepare('SELECT count(*)::int count FROM attachment_access_events WHERE link_id=?').bind(portal.linkId).first('count');
 // The outer routing observation is genuinely non-Field. The controlled owner
 // handoff commits a Field marker before entering the real locked read unit.
 expect(await fieldFileWriterOwnerInDatabase(db,actor,{kind:'link',id:portal.linkId})).toBe('other');
 routeState.portalReader.mockImplementationOnce(async(owner:typeof db,_actor:unknown,_link:string,_route:unknown,read:(unit:import('@erpx/server-runtime/public/runtime/types').SqlDatabase)=>Promise<unknown>)=>{
  await owner.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft','tester',1,100)").bind(`${prefix}-late-marker`,field,portal.sourceId).run();
  return owner.transaction(async unit=>{const {attachmentPrivateReadSourceLocks}=await import('@erpx/server-files/public/private-attachment-reads');await unit.batch([attachmentPrivateReadSourceLocks(unit)]);return read(unit);});
 });
 await expect(actual.attachmentContent(db,store,current,portal.linkId,'original')).rejects.toMatchObject({status:503,code:'FIELD_FILES_NATIVE_REQUIRED'});expect(routeState.portalReader).toHaveBeenCalledOnce();
 routeState.portalReader.mockImplementationOnce(async(owner:typeof db,_actor:unknown,_link:string,_route:unknown,read:(unit:import('@erpx/server-runtime/public/runtime/types').SqlDatabase)=>Promise<unknown>)=>owner.transaction(async unit=>{const {attachmentPrivateReadSourceLocks}=await import('@erpx/server-files/public/private-attachment-reads');await unit.batch([attachmentPrivateReadSourceLocks(unit)]);return read(unit);}));
 await expect(actual.portalAttachmentContentForRequest(db,store,current,portal.linkId,async()=>{})).rejects.toMatchObject({status:503,code:'FIELD_FILES_NATIVE_REQUIRED'});
 const request={sourceLocks:()=>[],assertCurrent:async()=>{}};
 for(const read of [{kind:'status' as const,linkId:portal.linkId},{kind:'sets' as const,input:{targetType:'customer_access_binding' as const,targetId:`${prefix}-portal`,purposeKey:'customer_support_evidence' as const,componentKind:'action' as const,expectedTargetVersion:1}}])await expect(actual.portalAttachmentMetadataReadForRequest(db,current,read,request)).rejects.toMatchObject({status:503,code:'FIELD_FILES_NATIVE_REQUIRED'});
 expect(get).not.toHaveBeenCalled();expect(await db.prepare('SELECT count(*)::int count FROM attachment_access_events WHERE link_id=?').bind(portal.linkId).first('count')).toBe(before);
});
