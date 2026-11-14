/**
 * Retained Brand identity read contract.
 *
 * Brand draft/publish/rollback/lock/cancel writes are owned by the native Rust
 * command and HTTP path. Their lifecycle, CAS, replay, independent-publication,
 * media and authority negatives are asserted by:
 *   - tests/integration/brand/rust-brand-identity-postgres.test.rs
 *   - tests/integration/brand/rust-brand-identity-http-postgres.test.rs
 *   - tests/unit/brand/rust-brand-identity-policy.test.rs
 * This suite keeps the registered workflow titles for the live TypeScript
 * consumers: the published identity, Brand workspace, storefront projection and
 * independent storefront-edit guard. Rows are seeded as fixture state, never by
 * a TypeScript identity writer.
 */
import type {BrandIdentity} from "@erpx/server-brand/public/brand-identity";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {brandIdentityWorkspace,publishedBrandIdentity} from "@erpx/server-brand/public/brand-identity";
import {readStorefrontAppBrand} from "@erpx/server-brand/public/branding";
import {frontCompanyBrand} from "@erpx/server-work_projection/public/front-app";
import {assertBrandCategoryLocks} from "@erpx/server-brand/domain/policy";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env,holdingBindingBySlug:async(holdingSlug:string)=>({id:"h001",slug:holdingSlug})}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
const identity=(shortName="Engineering demo",accentColor="#224488",logoPublicationId:string|null=null):BrandIdentity=>({identity:{shortName,tagline:"Approved company identity"},visual:{accentColor,logoPublicationId}});
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role="hr_admin";f.head.assignmentId=await f.grant(f.head,"hr_admin");for(const actor of [f.owner,f.head,f.claimant])actor.modules=["brand"];});
type SeededDraft={id:string;identity:BrandIdentity;createdBy:string;version:number};
async function seedEvent(recordId:string,action:string,version:number,actor:string,details:unknown,at:number){
 await db.prepare("INSERT INTO brand_identity_events(id,company_id,record_id,action,version,actor_id,reason,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`brand-event-${key()}`,f.company,recordId,action,version,actor,`Fixture ${action} event`,JSON.stringify(details),at).run();
}
async function seedDraft(input:{shortName?:string;accentColor?:string;logoPublicationId?:string|null;actor?:WorkContext;id?:string;basedRevision?:number;version?:number;status?:"draft"|"cancelled";at?:number}={}):Promise<SeededDraft>{
 const at=input.at??Date.now(),actor=input.actor??f.owner,id=input.id??`brand-draft-${key()}`,value=identity(input.shortName,input.accentColor??"#224488",input.logoPublicationId??null),version=input.version??1;
 await db.batch([
  db.prepare("INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,status,version,created_by,updated_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(id,f.company,input.basedRevision??0,JSON.stringify(value),input.status??"draft",version,actor.userId,actor.userId,at,at),
  db.prepare("INSERT INTO brand_identity_events(id,company_id,record_id,action,version,actor_id,reason,details_json,created_at) VALUES(?,?,?,'draft',1,?,?,?,?)").bind(`brand-event-${key()}`,f.company,id,actor.userId,"Fixture draft create",JSON.stringify({identity:value,basedRevision:input.basedRevision??0}),at),
 ]);
 return {id,identity:value,createdBy:actor.userId,version};
}
async function seedPublished(draft:SeededDraft,input:{revision?:number;publishedBy?:WorkContext;publishedVersion?:number;at?:number}={}){
 const at=input.at??Date.now(),reviewer=input.publishedBy??f.head,revision=input.revision??1,publishedId=`brand-version-${key()}`,publishedVersion=input.publishedVersion??revision;
 await db.batch([
  db.prepare("INSERT INTO brand_identity_versions(id,company_id,revision,identity_json,draft_id,draft_version,created_by,published_by,published_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(publishedId,f.company,revision,JSON.stringify(draft.identity),draft.id,draft.version,draft.createdBy,reviewer.userId,at),
  db.prepare("UPDATE brand_identity_drafts SET status='published',version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(reviewer.userId,at,draft.id),
  db.prepare("INSERT INTO brand_identity_active(company_id,identity_version_id,version,updated_by,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET identity_version_id=excluded.identity_version_id,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(f.company,publishedId,publishedVersion,reviewer.userId,at),
  db.prepare("INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_publication_id,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET short_name=excluded.short_name,accent_color=excluded.accent_color,logo_publication_id=excluded.logo_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at").bind(f.company,draft.identity.identity.shortName,draft.identity.visual.accentColor,draft.identity.visual.logoPublicationId,reviewer.userId,at),
 ]);
 await seedEvent(publishedId,"publish",publishedVersion,reviewer.userId,{draftId:draft.id,draftVersion:draft.version,revision,identity:draft.identity},at);
 return {publishedId,revision,publishedVersion};
}
async function seedRollback(publishedId:string,identityValue:BrandIdentity,input:{publishedVersion:number;at?:number}){
 const at=input.at??Date.now();
 await db.batch([
  db.prepare("UPDATE brand_identity_active SET identity_version_id=?,version=?,updated_by=?,updated_at=? WHERE company_id=?").bind(publishedId,input.publishedVersion,f.owner.userId,at,f.company),
  db.prepare("UPDATE storefront_app_brands SET short_name=?,accent_color=?,logo_publication_id=?,updated_by=?,updated_at=? WHERE company_id=?").bind(identityValue.identity.shortName,identityValue.visual.accentColor,identityValue.visual.logoPublicationId,f.owner.userId,at,f.company),
 ]);
 await seedEvent(publishedId,"rollback",input.publishedVersion,f.owner.userId,{identity:identityValue},at);
}
async function seedMedia(state:"draft"|"approved"){
 const token=key(),setId=`brand-media-set-${token}`,linkId=`brand-media-link-${token}`,publicationId=`brand-media-${token}`,revision=state==="approved"?1:2;
 await db.batch([
  db.prepare("INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES(?,?, 'company_storefront_brand',?,'publication','logo',?,1,'public','[]',?,?,?)").bind(setId,f.company,f.company,revision,f.owner.userId,1,1),
  db.prepare("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES(?,?,'draft',?,?,?)").bind(linkId,setId,f.owner.userId,1,9007199254740000),
  db.prepare("INSERT INTO attachment_publications(id,company_id,source_link_id,target_type,target_id,derivative_object_key,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,revision,state,created_by,created_at,approved_by,approved_at) VALUES(?,?,?,'company_storefront_brand',?,?,'image/webp','checksum',10,'Fixture approved logo',?,?,?,?,?,?)").bind(publicationId,f.company,linkId,f.company,`${token}.webp`,revision,state,f.owner.userId,1,state==="approved"?f.head.userId:null,state==="approved"?2:null),
 ]);
 return publicationId;
}
async function seedLock(category:"identity"|"visual",locked:boolean,input:{version:number;at?:number;identity?:BrandIdentity}){
 const at=input.at??Date.now(),snapshot=(input.identity??identity())[category];
 await db.prepare("INSERT INTO brand_category_locks(company_id,category,locked,approved_snapshot_json,version,updated_by,reason,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(company_id,category) DO UPDATE SET locked=excluded.locked,approved_snapshot_json=excluded.approved_snapshot_json,version=excluded.version,updated_by=excluded.updated_by,reason=excluded.reason,updated_at=excluded.updated_at").bind(f.company,category,locked,JSON.stringify(snapshot),input.version,f.owner.userId,"Fixture approved category review",at).run();
 return at;
}
it("saves and previews a versioned draft without changing the published identity and publishes it independently",async()=>{
 const at=Date.now(),before=await readStorefrontAppBrand(db,f.owner),draft=await seedDraft({at});
 let data=await brandIdentityWorkspace(f.owner);expect(data.drafts).toHaveLength(1);expect(JSON.parse(data.drafts[0].identity_json)).toEqual(identity());expect(data.active).toBeNull();expect(await readStorefrontAppBrand(db,f.owner)).toEqual(before);
 await db.prepare("UPDATE brand_identity_drafts SET identity_json=?,version=version+1,updated_by=?,updated_at=? WHERE id=?").bind(JSON.stringify(identity("Reviewed demo name")),f.owner.userId,at+1,draft.id).run();await seedEvent(draft.id,"save",2,f.owner.userId,{identity:identity("Reviewed demo name")},at+1);
 const published=await seedPublished({...draft,identity:identity("Reviewed demo name"),version:2},{revision:1,publishedBy:f.head,publishedVersion:1,at:at+2});
 data=await brandIdentityWorkspace(f.owner);expect(data.active).toMatchObject({revision:1,active_version:1,identity:identity("Reviewed demo name")});expect(data.versions).toHaveLength(1);expect(data.drafts[0]).toMatchObject({status:"published",version:3});
 expect(data.history.map(event=>event.action)).toEqual(["publish","save","draft"]);
 expect(await readStorefrontAppBrand(db,f.owner)).toMatchObject({shortName:"Reviewed demo name",accentColor:"#224488",identityRevision:1});
 const slug=String(await db.prepare("SELECT slug FROM organization_nodes WHERE id=?").bind(f.company).first("slug"));
 expect(await frontCompanyBrand(f.owner.holdingSlug,slug)).toMatchObject({shortName:"Reviewed demo name",accentColor:"#224488",logoPublicationId:null});
 expect(await publishedBrandIdentity(db,f.company)).toMatchObject({id:published.publishedId,revision:1,active_version:1,identity:identity("Reviewed demo name")});
 await expect(db.prepare("UPDATE brand_identity_versions SET identity_json='{}' WHERE id=?").bind(published.publishedId).run()).rejects.toThrow("BRAND_IDENTITY_EVIDENCE_IMMUTABLE");
});
it("locks approved categories against draft saves and publication until an owner explicitly unlocks them",async()=>{
 const draft=await seedDraft({shortName:"Engineering demo"});await seedPublished(draft,{revision:1,publishedVersion:1});
 await seedLock("visual",true,{version:1});let data=await brandIdentityWorkspace(f.owner);expect(data.locks[0]).toMatchObject({category:"visual",locked:true,version:1});
 const altered={...identity(),visual:{accentColor:"#778899",logoPublicationId:null}};
 let lockedError:unknown;try{assertBrandCategoryLocks(data.locks,altered);}catch(error){lockedError=error;}
 expect(lockedError).toMatchObject({status:409,code:"BRAND_CATEGORY_LOCKED"});
 expect(()=>assertBrandCategoryLocks(data.locks,identity())).not.toThrow();
 expect(data.canLock).toBe(true);expect((await brandIdentityWorkspace(f.head)).canLock).toBe(false);
 await seedLock("visual",false,{version:2});data=await brandIdentityWorkspace(f.owner);expect(data.locks[0]).toMatchObject({category:"visual",locked:false,version:2});
 const permitted=await seedDraft({shortName:"New short name only",basedRevision:1}),second=await seedPublished(permitted,{revision:2,publishedVersion:2});
 expect(await publishedBrandIdentity(db,f.company)).toMatchObject({id:second.publishedId,revision:2,active_version:2,identity:identity("New short name only")});
 expect((await brandIdentityWorkspace(f.owner)).active).toMatchObject({active_version:2,identity:{visual:{accentColor:"#224488"}}});
});
it("rolls back to immutable published versions with current locks, versions and replay-safe history",async()=>{
 const firstDraft=await seedDraft({shortName:"Engineering demo"}),first=await seedPublished(firstDraft,{revision:1,publishedVersion:1});
 const secondDraft=await seedDraft({shortName:"Second identity",accentColor:"#667788",basedRevision:1}),second=await seedPublished(secondDraft,{revision:2,publishedVersion:2});
 await seedLock("visual",true,{version:1});await seedLock("visual",false,{version:2});
 await seedRollback(first.publishedId,identity(),{publishedVersion:3});
 const data=await brandIdentityWorkspace(f.owner);expect(data.active).toMatchObject({id:first.publishedId,revision:1,active_version:3,identity:identity()});expect(data.versions).toHaveLength(2);expect(data.history.filter(event=>event.action==="rollback")).toHaveLength(1);
 expect(await readStorefrontAppBrand(db,f.owner)).toMatchObject({shortName:"Engineering demo",accentColor:"#224488"});
 expect(await publishedBrandIdentity(db,f.company)).toMatchObject({id:first.publishedId,revision:1,active_version:3});
 expect(second.publishedId).not.toBe(first.publishedId);
 await expect(db.prepare("DELETE FROM brand_identity_events WHERE company_id=?").bind(f.company).run()).rejects.toThrow("BRAND_IDENTITY_EVIDENCE_IMMUTABLE");
});
it("allows only one concurrent publication and requires reviewing a stale draft against the current identity",async()=>{
 const first=await seedDraft({shortName:"Owner draft"}),second=await seedDraft({shortName:"HR draft",actor:f.head});
 const published=await seedPublished(first,{revision:1,publishedVersion:1});
 await expect(db.prepare("INSERT INTO brand_identity_versions(id,company_id,revision,identity_json,draft_id,draft_version,created_by,published_by,published_at) VALUES(?,?,1,?,?,1,?,?,?)").bind(`brand-version-${key()}`,f.company,JSON.stringify(second.identity),second.id,second.createdBy,f.owner.userId,Date.now()).run()).rejects.toThrow();
 const remaining=(await brandIdentityWorkspace(f.owner)).drafts.find(row=>row.status==="draft")!;
 expect(remaining.id).toBe(second.id);expect(Number(remaining.based_revision)).toBe(0);
 await db.prepare("UPDATE brand_identity_drafts SET based_revision=1,version=version+1 WHERE id=?").bind(remaining.id).run();await seedEvent(remaining.id,"save",2,remaining.created_by,{basedRevision:1},Date.now());
 const rebased=await seedPublished({id:remaining.id,identity:second.identity,createdBy:second.createdBy,version:2},{revision:2,publishedVersion:2,publishedBy:f.owner});
 expect((await brandIdentityWorkspace(f.owner)).versions.map(row=>row.revision)).toEqual([2,1]);
 expect(published.publishedId).not.toBe(rebased.publishedId);
});
it("cancels drafts without deleting evidence and refuses foreign media, scopes and revoked authority",async()=>{
 const cancelled=await seedDraft({shortName:"Withdrawn identity",status:"cancelled",version:2});
 await seedEvent(cancelled.id,"cancel",2,f.owner.userId,{},Date.now());
 let data=await brandIdentityWorkspace(f.owner);expect(data.drafts[0]).toMatchObject({status:"cancelled",version:2});expect(data.history.filter(event=>event.action==="cancel")).toHaveLength(1);
 expect((await brandIdentityWorkspace(f.claimant)).drafts).toEqual([]);expect((await brandIdentityWorkspace(f.claimant)).history).toEqual([]);
 const owner=await seedDraft({id:`brand-draft-${key()}`});await seedPublished(owner,{revision:1,publishedVersion:1});
 const approvedMedia=await seedMedia("approved"),draftMedia=await seedMedia("draft");
 expect((await brandIdentityWorkspace(f.owner)).media.map(row=>row.id)).toEqual([approvedMedia]);expect(draftMedia).not.toBe(approvedMedia);
 await expect(brandIdentityWorkspace({...f.head,companyId:"foreign"})).rejects.toMatchObject({status:403});
 await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.owner.assignmentId).run();
 await expect(brandIdentityWorkspace(f.owner)).rejects.toMatchObject({code:"BRAND_AUTHORITY_CHANGED"});
 data=await brandIdentityWorkspace(f.head);expect(data.active).toMatchObject({revision:1});
});
