import type {BrandIdentity} from "@erpx/server-brand/public/brand-identity";
import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {brandIdentityWorkspace,saveBrandIdentityDraft,publishBrandIdentity,rollbackBrandIdentity,changeBrandCategoryLock,cancelBrandDraft,publishedBrandIdentity} from "@erpx/server-brand/public/brand-identity";
import {readStorefrontAppBrand,updateStorefrontAppBrand} from "@erpx/server-brand/public/branding";
import {frontCompanyBrand} from "@erpx/server-work_projection/public/front-app";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env,holdingBindingBySlug:async(holdingSlug:string)=>({id:"h001",slug:holdingSlug})}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
const identity=(shortName="Engineering demo"):BrandIdentity=>({identity:{shortName,tagline:"Approved company identity"},visual:{accentColor:"#224488",logoPublicationId:null}});
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role="hr_admin";f.head.assignmentId=await f.grant(f.head,"hr_admin");for(const actor of [f.owner,f.head,f.claimant])actor.modules=["brand"];});
async function draft(value=identity(),actor=f.owner,expectedPublishedVersion=0){return saveBrandIdentityDraft(actor,{expectedPublishedVersion,identity:value,reason:"Reviewable identity draft with approved values"},key());}
async function published(value=identity()){const created=await draft(value);return publishBrandIdentity(f.head,String(created.id),{expectedVersion:1,expectedPublishedVersion:0,reason:"Independent review of customer-facing identity"},key());}
it("saves and previews a versioned draft without changing the published identity and publishes it independently",async()=>{
 const before=await readStorefrontAppBrand(db,f.owner),body={expectedPublishedVersion:0,identity:identity(),reason:"Draft approved brand values for review"},command=key(),created=await saveBrandIdentityDraft(f.owner,body,command),id=String(created.id);
 expect(await saveBrandIdentityDraft(f.owner,body,command)).toEqual(created);
 let data=await brandIdentityWorkspace(f.owner);expect(data.drafts).toHaveLength(1);expect(JSON.parse(data.drafts[0].identity_json)).toEqual(identity());expect(data.active).toBeNull();expect(await readStorefrontAppBrand(db,f.owner)).toEqual(before);
 await saveBrandIdentityDraft(f.owner,{id,expectedVersion:1,expectedPublishedVersion:0,identity:identity("Reviewed demo name"),reason:"Correct the name before publication"},key());
 await expect(saveBrandIdentityDraft(f.owner,{id,expectedVersion:1,expectedPublishedVersion:0,identity:identity("Obsolete name"),reason:"Stale editor must not overwrite"},key())).rejects.toMatchObject({status:409});
 await expect(publishBrandIdentity(f.owner,id,{expectedVersion:2,expectedPublishedVersion:0,reason:"Creator cannot approve their own identity"},key())).rejects.toMatchObject({code:"BRAND_INDEPENDENT_PUBLICATION"});
 const publishInput={expectedVersion:2,expectedPublishedVersion:0,reason:"Independent reviewer approves the corrected identity"},publishKey=key(),result=await publishBrandIdentity(f.head,id,publishInput,publishKey);expect(await publishBrandIdentity(f.head,id,publishInput,publishKey)).toEqual(result);
 data=await brandIdentityWorkspace(f.owner);expect(data.active).toMatchObject({revision:1,active_version:1,identity:identity("Reviewed demo name")});expect(data.versions).toHaveLength(1);expect(data.drafts[0]).toMatchObject({status:"published",version:3});
 expect(data.history.map(event=>event.action)).toEqual(["publish","save","draft"]);
 expect(await readStorefrontAppBrand(db,f.owner)).toMatchObject({shortName:"Reviewed demo name",accentColor:"#224488",identityRevision:1});
 const slug=String(await db.prepare("SELECT slug FROM organization_nodes WHERE id=?").bind(f.company).first("slug"));
 expect(await frontCompanyBrand(f.owner.holdingSlug,slug)).toMatchObject({shortName:"Reviewed demo name",accentColor:"#224488",logoPublicationId:null});
 await expect(updateStorefrontAppBrand(db,f.owner,{shortName:"Bypass draft",commandId:key()})).rejects.toMatchObject({code:"BRAND_PUBLISHED_IDENTITY_CONTROLLED"});
 await expect(db.prepare("UPDATE brand_identity_versions SET identity_json='{}' WHERE id=?").bind(result.publishedId).run()).rejects.toThrow("BRAND_IDENTITY_EVIDENCE_IMMUTABLE");
});
it("locks approved categories against draft saves and publication until an owner explicitly unlocks them",async()=>{
 await published();await changeBrandCategoryLock(f.owner,{category:"visual",locked:true,expectedVersion:0,expectedPublishedVersion:1,reason:"Protect independently approved colors"},key());
 const altered={...identity(),visual:{accentColor:"#778899",logoPublicationId:null}};
 await expect(draft(altered,f.head,1)).rejects.toMatchObject({code:"BRAND_CATEGORY_LOCKED"});
 await expect(changeBrandCategoryLock(f.head,{category:"visual",locked:false,expectedVersion:1,expectedPublishedVersion:1,reason:"HR cannot release the owner lock"},key())).rejects.toMatchObject({status:403});
 const permitted=await draft(identity("New short name only"),f.owner,1);await publishBrandIdentity(f.head,String(permitted.id),{expectedVersion:1,expectedPublishedVersion:1,reason:"Name revised while approved visual values remain locked"},key());
 expect((await publishedBrandIdentity(db,f.company))?.identity.visual.accentColor).toBe("#224488");
 const unlock={category:"visual",locked:false,expectedVersion:1,expectedPublishedVersion:2,reason:"Owner reviewed the planned new palette"},command=key(),result=await changeBrandCategoryLock(f.owner,unlock,command);expect(await changeBrandCategoryLock(f.owner,unlock,command)).toEqual(result);
 const changed=await draft(altered,f.owner,2);await publishBrandIdentity(f.head,String(changed.id),{expectedVersion:1,expectedPublishedVersion:2,reason:"Independently approved the unlocked visual revision"},key());
 expect((await brandIdentityWorkspace(f.owner)).locks[0]).toMatchObject({category:"visual",locked:false,version:2});
 expect((await publishedBrandIdentity(db,f.company))?.identity.visual.accentColor).toBe("#778899");
});
it("rolls back to immutable published versions with current locks, versions and replay-safe history",async()=>{
 const first=await published(),secondDraft=await draft({...identity("Second identity"),visual:{accentColor:"#667788",logoPublicationId:null}},f.owner,1);
 await publishBrandIdentity(f.head,String(secondDraft.id),{expectedVersion:1,expectedPublishedVersion:1,reason:"Second identity independently reviewed"},key());
 await changeBrandCategoryLock(f.owner,{category:"visual",locked:true,expectedVersion:0,expectedPublishedVersion:2,reason:"Protect the new approved palette"},key());
 const body={publishedId:String(first.publishedId),expectedPublishedVersion:2,reason:"Restore the previously approved identity"};
 await expect(rollbackBrandIdentity(f.owner,body,key())).rejects.toMatchObject({code:"BRAND_CATEGORY_LOCKED"});
 await changeBrandCategoryLock(f.owner,{category:"visual",locked:false,expectedVersion:1,expectedPublishedVersion:2,reason:"Owner reviewed the previous approved palette"},key());
 const command=key(),result=await rollbackBrandIdentity(f.owner,body,command);expect(await rollbackBrandIdentity(f.owner,body,command)).toEqual(result);
 const data=await brandIdentityWorkspace(f.owner);expect(data.active).toMatchObject({id:first.publishedId,revision:1,active_version:3,identity:identity()});expect(data.versions).toHaveLength(2);expect(data.history.filter(event=>event.action==="rollback")).toHaveLength(1);
 expect(await readStorefrontAppBrand(db,f.owner)).toMatchObject({shortName:"Engineering demo",accentColor:"#224488"});
 await expect(rollbackBrandIdentity(f.owner,body,key())).rejects.toMatchObject({status:409});
 await expect(db.prepare("DELETE FROM brand_identity_events WHERE company_id=?").bind(f.company).run()).rejects.toThrow("BRAND_IDENTITY_EVIDENCE_IMMUTABLE");
});
it("allows only one concurrent publication and requires reviewing a stale draft against the current identity",async()=>{
 const first=await draft(identity("Owner draft")),second=await draft(identity("HR draft"),f.head),results=await Promise.allSettled([publishBrandIdentity(f.head,String(first.id),{expectedVersion:1,expectedPublishedVersion:0,reason:"Independent review of owner draft"},key()),publishBrandIdentity(f.owner,String(second.id),{expectedVersion:1,expectedPublishedVersion:0,reason:"Independent review of HR draft"},key())]);
 expect(results.filter(result=>result.status==="fulfilled")).toHaveLength(1);expect((await brandIdentityWorkspace(f.owner)).versions).toHaveLength(1);
 const data=await brandIdentityWorkspace(f.owner),remaining=data.drafts.find(row=>row.status==="draft")!,reviewer=remaining.created_by===f.owner.userId?f.head:f.owner;
 await expect(publishBrandIdentity(reviewer,remaining.id,{expectedVersion:1,expectedPublishedVersion:1,reason:"Draft still based on obsolete identity"},key())).rejects.toMatchObject({code:"BRAND_DRAFT_REBASE_REQUIRED"});
 const maker=remaining.created_by===f.owner.userId?f.owner:f.head;await saveBrandIdentityDraft(maker,{id:remaining.id,expectedVersion:1,expectedPublishedVersion:1,identity:JSON.parse(remaining.identity_json),reason:"Review differences against current publication"},key());
 await publishBrandIdentity(reviewer,remaining.id,{expectedVersion:2,expectedPublishedVersion:1,reason:"Independently reviewed rebased identity"},key());expect((await brandIdentityWorkspace(f.owner)).versions).toHaveLength(2);
});
it("cancels drafts without deleting evidence and refuses foreign media, scopes and revoked authority",async()=>{
 await expect(draft({...identity(),visual:{accentColor:"#224488",logoPublicationId:"foreign-or-unapproved-publication"}})).rejects.toMatchObject({status:409});
 expect((await brandIdentityWorkspace(f.owner)).drafts).toEqual([]);
 const created=await draft(),id=String(created.id),body={expectedVersion:1,reason:"Withdraw the draft before review"},command=key(),result=await cancelBrandDraft(f.owner,id,body,command);expect(await cancelBrandDraft(f.owner,id,body,command)).toEqual(result);
 expect((await brandIdentityWorkspace(f.owner)).drafts[0]).toMatchObject({status:"cancelled",version:2});
 await expect(saveBrandIdentityDraft(f.owner,{id,expectedVersion:2,expectedPublishedVersion:0,identity:identity(),reason:"Cannot overwrite withdrawn draft"},key())).rejects.toMatchObject({code:"BRAND_DRAFT_UNAVAILABLE"});
 expect((await brandIdentityWorkspace(f.claimant)).drafts).toEqual([]);await expect(draft(identity(),f.claimant)).rejects.toMatchObject({status:403});
 await expect(publishBrandIdentity({...f.head,companyId:"foreign"},id,{expectedVersion:1,expectedPublishedVersion:0,reason:"Different company cannot publish"},key())).rejects.toMatchObject({status:403});
 await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.owner.assignmentId).run();await expect(brandIdentityWorkspace(f.owner)).rejects.toMatchObject({code:"BRAND_AUTHORITY_CHANGED"});await expect(draft()).rejects.toMatchObject({status:403});
});
