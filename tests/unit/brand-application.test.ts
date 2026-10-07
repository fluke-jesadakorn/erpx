import { describe, expect, test } from "vitest";
import { createBrandIdentityApplication, type BrandIdentityPorts, type BrandMutation } from "../../src/server/modules/brand/application/identity";
import { brandIdentitySchema, type ActiveBrandIdentity, type BrandCategoryLock, type BrandDraft } from "../../src/server/modules/brand/domain/identity";

const identity = { identity: { shortName: "ERPX", tagline: "Approved identity" }, visual: { accentColor: "#224488", logoPublicationId: null } };
const draft: BrandDraft = { id: "draft-1", company_id: "company-1", based_revision: 0, identity_json: JSON.stringify(identity), status: "draft", version: 1, created_by: "maker", updated_by: "editor", updated_at: 10 };
const reason = "Independently review the identity";
function fixture(overrides: Partial<BrandIdentityPorts> = {}) {
  const mutations: BrandMutation[] = [];
  const calls: string[] = [];
  const ports: BrandIdentityPorts = {
    companyId: "company-1", clock: { now: () => 42 }, isSamePerson: (id) => id === "reviewer",
    async authorize(_write, owner) { calls.push(owner ? "authorize-owner" : "authorize"); },
    async replay() { calls.push("replay"); return null; },
    async active() { calls.push("active"); return null; },
    async locks() { calls.push("locks"); return []; },
    async draft() { calls.push("draft"); return draft; },
    async published() { return null; }, async nextRevision() { return 1; },
    async commit(mutation, result) { mutations.push(mutation); return result; },
    ...overrides,
  };
  return { app: createBrandIdentityApplication(ports), mutations, calls };
}

describe("Brand application through semantic ports", () => {
  test("normalizes identity input, selects save command/version and supplies the injected clock", async () => {
    const f = fixture();
    const result = await f.app.save({ expectedPublishedVersion: 0, identity: { ...identity, visual: { ...identity.visual, accentColor: "#AABBCC" } }, reason }, "key-123456");
    expect(result).toEqual({ id: "brand-draft-key-123456", status: "draft", version: 1 });
    expect(f.mutations[0]).toMatchObject({ action: "save", operation: "brand.identity.draft", now: 42, identity: { visual: { accentColor: "#aabbcc" } } });
    expect(f.calls).toEqual(["authorize", "replay", "active", "locks"]);
  });
  test("replays before stale state evaluation and does not repeat writes", async () => {
    const f = fixture({ async replay() { return { id: "draft-1", version: 2 }; }, async active() { throw new Error("must not read changed state after replay"); } });
    expect(await f.app.publish("draft-1", { expectedVersion: 1, expectedPublishedVersion: 0, reason }, "key-123456")).toEqual({ id: "draft-1", version: 2 });
    expect(f.mutations).toEqual([]);
  });
  test("requires another person even when principals differ", async () => {
    const f = fixture({ isSamePerson: (id) => id === "maker" });
    await expect(f.app.publish("draft-1", { expectedVersion: 1, expectedPublishedVersion: 0, reason }, "key-123456")).rejects.toMatchObject({ status: 403, code: "BRAND_INDEPENDENT_PUBLICATION" });
    expect(f.mutations).toEqual([]);
  });
  test("rejects an edited locked category and stale published revision without a commit", async () => {
    const locks: BrandCategoryLock[] = [{ category: "visual", locked: true, approved_snapshot_json: JSON.stringify(identity.visual), version: 1, reason, updated_by: "owner", updated_at: 10 }];
    const f = fixture({ async locks() { return locks; } });
    await expect(f.app.save({ expectedPublishedVersion: 0, identity: { ...identity, visual: { ...identity.visual, accentColor: "#778899" } }, reason }, "key-123456")).rejects.toMatchObject({ code: "BRAND_CATEGORY_LOCKED" });
    const active = { revision: 2, active_version: 2, identity } as ActiveBrandIdentity;
    const stale = fixture({ async active() { return active; } });
    await expect(stale.app.publish("draft-1", { expectedVersion: 1, expectedPublishedVersion: 2, reason }, "key-123456")).rejects.toMatchObject({ code: "BRAND_DRAFT_REBASE_REQUIRED" });
    expect([...f.mutations, ...stale.mutations]).toEqual([]);
  });
  test("supplies immutable publication snapshots and expected versions to atomic adapter guards", async () => {
    const f = fixture();
    await f.app.publish("draft-1", { expectedVersion: 1, expectedPublishedVersion: 0, reason }, "key-123456");
    expect(f.mutations[0]).toMatchObject({ action: "publish", draft, identity, expectedVersion: 1, expectedPublishedVersion: 0, revision: 1, publishedId: "brand-version-key-123456", version: 2, now: 42 });
  });
  test("requires owner authorization and an existing approved identity before locking", async () => {
    const f = fixture();
    await expect(f.app.changeLock({ category: "visual", locked: true, expectedVersion: 0, expectedPublishedVersion: 1, reason }, "key-123456")).rejects.toMatchObject({ code: "BRAND_PUBLISHED_IDENTITY_REQUIRED" });
    expect(f.calls[0]).toBe("authorize-owner");
    expect(f.mutations).toEqual([]);
  });
  test("keeps strict customer-facing identity shape", () => {
    expect(brandIdentitySchema.safeParse({ ...identity, implementation: "hidden" }).success).toBe(false);
  });
});
import { createBrandingApplication, normalizeBrandMedia, type BrandingPorts, type BrandMutation as BrandingMutation } from "../../src/server/modules/brand/application/branding";
import { readBrandIdentityWorkspace } from "../../src/server/modules/brand/application/identity-read";
import { storefrontPresentation, type BrandProfileRow } from "../../src/server/modules/brand/domain/branding";

function brandingFixture(overrides: Partial<BrandingPorts<Uint8Array>> = {}) {
 const calls:string[]=[],mutations:BrandingMutation[]=[];
 const ports:BrandingPorts<Uint8Array>={
  actor:{holdingId:"holding",holdingName:"Holding Name",holdingSlug:"holding-slug",companyId:"company",branchId:"branch"},clock:{now:()=>42},legacyAttachmentsReadable:false,
  urls:{attachment:id=>id?`attachment/${id}`:null,publication:id=>id?`publication/${id}`:null},
  async holdingAuthority(){return true;},async organizationAuthority(){return true;},async evidenceAuthority(){return true;},assertMediaWritable(){calls.push("writable");},
  async company(){return {display_name:"Company"};},async branch(id){return {id,display_name:"North Branch"};},async companyExists(){return true;},
  async profile(){return {primary_attachment_id:"old-media"} as BrandProfileRow;},async branches(){return [];},async profiles(){return [];},
  async storefront(){return {slug:"company-slug",display_name:"Company",short_name:"Company",accent_color:"#3659c9",logo_attachment_id:"old-media",logo_publication_id:null};},
  async storefrontLogo(){return {logo_attachment_id:"old-media"};},async published(){return null;},async recordBranch(){return "branch";},async evidence(){return null;},
  image:{async metadata(){return {width:64,height:64};},async encode(){return new Uint8Array([1,2,3]);}},async checksum(){return "checksum";},
  async attachmentObjectKey(){return "old-key";},
  files:{async put(key){calls.push(`put:${key}`);},async delete(key){calls.push(`delete:${key}`);}},
  async commit(mutation,result){mutations.push(mutation);calls.push("commit");return result;},...overrides,
 };
 return {app:createBrandingApplication(ports),ports,calls,mutations};
}
describe("Brand media and projection application",()=>{
 test("validates metadata through the provider port and retains provider encoding failures",async()=>{
  const invalid=brandingFixture({image:{async metadata(){throw new Error("bad decoder");},async encode(){return new Uint8Array();}}});
  await expect(invalid.app.normalize(new ArrayBuffer(0))).rejects.toMatchObject({code:"LOGO_INVALID"});
  await expect(normalizeBrandMedia({async metadata(){return {width:31,height:32};},async encode(){throw new Error("must not encode");}},new ArrayBuffer(0))).rejects.toMatchObject({code:"LOGO_DIMENSIONS"});
  const encode=brandingFixture({image:{async metadata(){return {width:64,height:64};},async encode(){throw new Error("encoder failure");}}});
  await expect(encode.app.normalize(new ArrayBuffer(0))).rejects.toThrow("encoder failure");
 });
 test("commits file replacement facts then cleans the previous object",async()=>{
  const f=brandingFixture();
  await f.app.completeUpload({id:"intent",owner_type:"branch_brand",owner_id:"branch",purpose:"primary",revision:1,original_name:"logo.png"},new ArrayBuffer(0));
  expect(f.mutations[0]).toMatchObject({action:"upload-profile",previousId:"old-media",attachmentId:"attachment-intent",bytes:3,now:42});
  expect(f.calls).toEqual(["writable","put:holding/branding/branch/branch/primary-intent.webp","commit","delete:old-key"]);
 });
 test("compensates a failed atomic write without deleting the previous object",async()=>{
  const f=brandingFixture({async commit(){throw new Error("authority revoked");}});
  await expect(f.app.completeUpload({id:"intent",owner_type:"company_storefront_brand",owner_id:"company",purpose:"logo",revision:1,original_name:"logo.png"},new ArrayBuffer(0))).rejects.toThrow("authority revoked");
  expect(f.calls).toEqual(["writable","put:holding/branding/storefront/company/logo-intent.webp","delete:holding/branding/storefront/company/logo-intent.webp"]);
 });
 test("checks media rollback before decoding and published identity before legacy edits",async()=>{
  const readonly=brandingFixture({assertMediaWritable(){throw new Error("read only");},image:{async metadata(){throw new Error("must not decode");},async encode(){return new Uint8Array();}}});
  await expect(readonly.app.completeUpload({id:"intent",owner_type:"branch_brand",owner_id:"branch",purpose:"primary",revision:1,original_name:"logo.png"},new ArrayBuffer(0))).rejects.toThrow("read only");
  const active={identity,revision:4} as ActiveBrandIdentity,published=brandingFixture({async published(){return active;},assertMediaWritable(){throw new Error("must not check rollback");}});
  await expect(published.app.updateStorefront({removeLogo:true,commandId:"key"})).rejects.toMatchObject({code:"BRAND_PUBLISHED_IDENTITY_CONTROLLED"});
 });
 test("published absence of a logo suppresses every legacy fallback",()=>{
  const f=brandingFixture(),current={slug:"company",display_name:"Company",short_name:"Old",accent_color:"#112233",logo_attachment_id:"legacy",logo_publication_id:"legacy-publication"};
  const brand=storefrontPresentation(f.ports.actor,current,{identity,revision:4} as ActiveBrandIdentity,f.ports.urls,false);
  expect(brand).toMatchObject({shortName:"ERPX",accentColor:"#224488",logoUrl:null,identityRevision:4});
 });
 test("document mode honors active branch defaults and a company-wide scope",async()=>{
  const f=brandingFixture({async profile(){return {document_mode:"both"} as BrandProfileRow;}});
  expect(await f.app.document({branchId:"branch"})).toMatchObject({mode:"both",branchName:"North Branch"});
  expect(await f.app.document({branchId:null,requestedMode:"branch"})).toMatchObject({mode:"holding",branch:null});
 });
 test("workspace view hides history and other's drafts using provided authority facts",async()=>{
  const output=await readBrandIdentityWorkspace({actorId:"worker",canManage:false,canLock:false,async authorize(){},async drafts(includeOthers){expect(includeOthers).toBe(false);return [];},async versions(){return [];},async locks(){return [];},async history(){return [{id:"private"}] as never;},async active(){return null;},async legacy(){return {shortName:"Legacy",accentColor:"#123456"};},async media(){return [];}});
  expect(output).toMatchObject({history:[],initial:{identity:{shortName:"Legacy",tagline:""},visual:{logoPublicationId:null}},canManage:false});
 });
});
